use {
    litesvm_cpi_tree::{
        cpi_tree as parse_cpi_tree, format_cpi_tree as format_cpi_tree_original,
        format_cpi_tree_with as format_cpi_tree_with_original,
        ComputeUnits as ComputeUnitsOriginal, CpiFrame as CpiFrameOriginal,
        CpiOutcome as CpiOutcomeOriginal, FrameLog as FrameLogOriginal,
    },
    napi::bindgen_prelude::*,
    solana_address::Address,
    std::{
        collections::{hash_map::Entry, HashMap},
        str::FromStr,
    },
};

/// CU values are JS numbers, not bigints, so the whole tree survives
/// `JSON.stringify`. Exact up to 2^53; only `availableAtStart` can
/// exceed that (see its doc).
#[napi(object)]
pub struct CpiComputeUnits {
    /// CU consumed by this frame, cumulative over its children.
    pub consumed: f64,
    /// CU remaining in the transaction budget when this frame started.
    /// This echoes the configured budget rather than measuring work, and
    /// LiteSVM accepts budgets up to u64::MAX (`ComputeBudget.computeUnitLimit`),
    /// so values above 2^53 are reachable and round to the nearest
    /// representable number here.
    pub available_at_start: f64,
}

#[napi(discriminant = "type", discriminant_case = "lowercase")]
pub enum CpiOutcome {
    Success,
    Failed { message: Option<String> },
    Truncated,
}

#[napi(discriminant = "type", discriminant_case = "lowercase")]
pub enum CpiFrameLog {
    Msg { value: String },
    Data { value: String },
}

#[napi(object)]
pub struct CpiFrame {
    /// Base58 program address, as it appears in the log lines.
    pub program_id: String,
    pub outcome: CpiOutcome,
    pub compute_units: Option<CpiComputeUnits>,
    pub instruction_name: Option<String>,
    pub logs: Vec<CpiFrameLog>,
    pub children: Vec<CpiFrame>,
}

impl From<ComputeUnitsOriginal> for CpiComputeUnits {
    fn from(value: ComputeUnitsOriginal) -> Self {
        // u64 -> f64 rounds above 2^53. `consumed` can't get there (it
        // counts executed instructions), but `available_at_start` can
        // when the budget is cranked past 2^53; harmless rounding.
        Self {
            consumed: value.consumed as f64,
            available_at_start: value.available_at_start as f64,
        }
    }
}

impl From<CpiOutcomeOriginal> for CpiOutcome {
    fn from(value: CpiOutcomeOriginal) -> Self {
        match value {
            CpiOutcomeOriginal::Success => Self::Success,
            CpiOutcomeOriginal::Failed { message } => Self::Failed { message },
            CpiOutcomeOriginal::Truncated => Self::Truncated,
        }
    }
}

impl From<FrameLogOriginal> for CpiFrameLog {
    fn from(value: FrameLogOriginal) -> Self {
        match value {
            FrameLogOriginal::Msg(value) => Self::Msg { value },
            FrameLogOriginal::Data(value) => Self::Data { value },
        }
    }
}

impl From<CpiFrameOriginal> for CpiFrame {
    fn from(value: CpiFrameOriginal) -> Self {
        Self {
            program_id: value.program_id.to_string(),
            outcome: value.outcome.into(),
            compute_units: value.compute_units.map(Into::into),
            instruction_name: value.instruction_name,
            logs: value.logs.into_iter().map(Into::into).collect(),
            children: value.children.into_iter().map(Into::into).collect(),
        }
    }
}

/// Parse Solana transaction logs into a CPI call tree.
#[napi]
pub fn cpi_tree(logs: Vec<String>) -> Vec<CpiFrame> {
    parse_cpi_tree(&logs).into_iter().map(Into::into).collect()
}

impl From<CpiOutcome> for CpiOutcomeOriginal {
    fn from(value: CpiOutcome) -> Self {
        match value {
            CpiOutcome::Success => Self::Success,
            CpiOutcome::Failed { message } => Self::Failed { message },
            CpiOutcome::Truncated => Self::Truncated,
        }
    }
}

impl From<CpiFrameLog> for FrameLogOriginal {
    fn from(value: CpiFrameLog) -> Self {
        match value {
            CpiFrameLog::Msg { value } => Self::Msg(value),
            CpiFrameLog::Data { value } => Self::Data(value),
        }
    }
}

// Fallible where the forward direction is not: `programId` is any JS
// string, and the CU numbers go back to the u64s they came from.
impl TryFrom<CpiFrame> for CpiFrameOriginal {
    type Error = Error;

    fn try_from(value: CpiFrame) -> Result<Self> {
        let program_id = Address::from_str(&value.program_id).map_err(|e| {
            Error::from_reason(format!("invalid program id '{}': {e}", value.program_id))
        })?;
        Ok(Self {
            program_id,
            outcome: value.outcome.into(),
            compute_units: value.compute_units.map(|cu| ComputeUnitsOriginal {
                consumed: cu.consumed as u64,
                available_at_start: cu.available_at_start as u64,
            }),
            instruction_name: value.instruction_name,
            logs: value.logs.into_iter().map(Into::into).collect(),
            children: value
                .children
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_>>()?,
        })
    }
}

fn convert_frames(frames: Vec<CpiFrame>) -> Result<Vec<CpiFrameOriginal>> {
    frames.into_iter().map(TryInto::try_into).collect()
}

/// Render CPI frames as `cargo tree`-style box art under `header`.
/// The header acts as a visible parent so a transaction's multiple
/// top-level frames read as siblings.
#[napi]
pub fn format_cpi_tree(header: String, frames: Vec<CpiFrame>) -> Result<String> {
    Ok(format_cpi_tree_original(&header, &convert_frames(frames)?))
}

/// Like `formatCpiTree`, but `programLabel` decides how each frame's
/// program id is rendered (an alias, a hyperlink, ...). Called once per
/// distinct program id, in tree order.
#[napi]
pub fn format_cpi_tree_with(
    header: String,
    frames: Vec<CpiFrame>,
    program_label: Function<String, String>,
) -> Result<String> {
    let frames = convert_frames(frames)?;
    // The crate's label hook is infallible; a JS callback is not. Resolve
    // every label up front so a throwing callback surfaces as an error
    // instead of unwinding mid-render.
    let mut labels = HashMap::new();
    collect_labels(&frames, &program_label, &mut labels)?;
    Ok(format_cpi_tree_with_original(&header, &frames, &|addr| {
        labels[addr].clone()
    }))
}

pub(crate) fn collect_labels(
    frames: &[CpiFrameOriginal],
    program_label: &Function<String, String>,
    labels: &mut HashMap<Address, String>,
) -> Result<()> {
    for frame in frames {
        if let Entry::Vacant(entry) = labels.entry(frame.program_id) {
            entry.insert(program_label.call(frame.program_id.to_string())?);
        }
        collect_labels(&frame.children, program_label, labels)?;
    }
    Ok(())
}
