use litesvm_cpi_tree::{
    cpi_tree as parse_cpi_tree, ComputeUnits as ComputeUnitsOriginal, CpiFrame as CpiFrameOriginal,
    CpiOutcome as CpiOutcomeOriginal, FrameLog as FrameLogOriginal,
};

/// CU values are JS numbers, not bigints, so the whole tree survives
/// `JSON.stringify`. Exact up to 2^53; only `availableAtStart` can
/// exceed that (see its doc).
#[napi(object, object_to_js = true, object_from_js = false)]
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

#[napi(
    object_to_js = true,
    object_from_js = false,
    discriminant = "type",
    discriminant_case = "lowercase"
)]
pub enum CpiOutcome {
    Success,
    Failed { message: Option<String> },
    Truncated,
}

#[napi(
    object_to_js = true,
    object_from_js = false,
    discriminant = "type",
    discriminant_case = "lowercase"
)]
pub enum CpiFrameLog {
    Msg { value: String },
    Data { value: String },
}

#[napi(object, object_to_js = true, object_from_js = false)]
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
