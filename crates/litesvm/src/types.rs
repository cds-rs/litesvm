use {
    crate::format_logs::format_logs,
    solana_account::AccountSharedData,
    solana_address::Address,
    solana_instruction::error::InstructionError,
    solana_message::inner_instruction::InnerInstructionsList,
    solana_program_error::ProgramError,
    solana_signature::Signature,
    solana_transaction_context::TransactionReturnData,
    solana_transaction_error::{TransactionError, TransactionResult as Result},
};

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransactionMetadata {
    #[cfg_attr(feature = "serde", serde(with = "crate::utils::serde_with_str"))]
    pub signature: Signature,
    pub logs: Vec<String>,
    pub inner_instructions: InnerInstructionsList,
    pub compute_units_consumed: u64,
    pub return_data: TransactionReturnData,
    pub fee: u64,

    /// Execution account frame for [`Self::inner_instructions`].
    ///
    /// Inner-instruction indices are execution-relative, not message-relative.
    /// Resolve them against this list.
    pub account_keys: Vec<Address>,
}

impl TransactionMetadata {
    pub fn pretty_logs(&self) -> String {
        format_logs(&self.logs)
    }

    /// Resolve an [`Self::inner_instructions`] account index against
    /// [`Self::account_keys`].
    ///
    /// Returns `None` for out-of-range indices; inner-instruction metadata is
    /// best-effort and may reference accounts unavailable to the caller.
    pub fn resolve_account(&self, index: u8) -> Option<&Address> {
        self.account_keys.get(index as usize)
    }

    /// The program a given inner instruction invoked, resolved against
    /// [`Self::account_keys`]. `outer` indexes the top-level instruction, `inner`
    /// the inner instruction beneath it. `None` if either index is out of range.
    pub fn inner_instruction_program(&self, outer: usize, inner: usize) -> Option<&Address> {
        let ix = self.inner_instructions.get(outer)?.get(inner)?;
        self.resolve_account(ix.instruction.program_id_index)
    }

    pub fn cpi_tree(&self) -> Vec<crate::cpi_tree::CpiFrame> {
        crate::cpi_tree::cpi_tree(&self.logs)
    }

    pub fn pretty_cpi_tree(&self) -> String {
        use crate::cpi_tree::{
            format_cpi_tree, transaction_compute_budget, transaction_total_cu, with_commas,
        };
        let frames = self.cpi_tree();
        // Same header agave's `solana logs --tree` builds: transaction-total
        // BPF CU and the budget, or an explicit no-data note. Never "0 CU":
        // native programs don't emit `consumed` lines, and reporting that
        // absence as zero would misstate the cost.
        let header = match (
            transaction_total_cu(&frames),
            transaction_compute_budget(&frames),
        ) {
            (Some(total), Some(budget)) => format!(
                "CPI Tree ({} BPF CU / {} budget):",
                with_commas(total),
                with_commas(budget)
            ),
            _ => "CPI Tree (no compute units in logs):".to_string(),
        };
        format_cpi_tree(&header, &frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_with_logs(logs: Vec<String>) -> TransactionMetadata {
        TransactionMetadata {
            logs,
            ..Default::default()
        }
    }

    #[test]
    fn pretty_cpi_tree_header_shows_total_and_budget() {
        let meta = meta_with_logs(vec![
            "Program GtdambwDgHWrDJdVPBkEHGhCwokqgAoch162teUjJse2 invoke [1]".to_string(),
            "Program GtdambwDgHWrDJdVPBkEHGhCwokqgAoch162teUjJse2 consumed 4817 of 1000000 \
             compute units"
                .to_string(),
            "Program GtdambwDgHWrDJdVPBkEHGhCwokqgAoch162teUjJse2 success".to_string(),
        ]);
        let out = meta.pretty_cpi_tree();
        assert!(
            out.starts_with("CPI Tree (4,817 BPF CU / 1,000,000 budget):"),
            "unexpected header: {out}"
        );
    }

    #[test]
    fn pretty_cpi_tree_header_notes_missing_cu() {
        // System-program-only transaction: native programs never emit
        // `consumed` lines, so there's no CU data to total.
        let meta = meta_with_logs(vec![
            "Program 11111111111111111111111111111111 invoke [1]".to_string(),
            "Program 11111111111111111111111111111111 success".to_string(),
        ]);
        let out = meta.pretty_cpi_tree();
        assert!(
            out.starts_with("CPI Tree (no compute units in logs):"),
            "unexpected header: {out}"
        );
    }

    #[test]
    fn inner_instruction_indices_resolve_against_account_keys_never_panicking() {
        use solana_message::{compiled_instruction::CompiledInstruction, inner_instruction::InnerInstruction};

        let loader = Address::new_from_array([10u8; 32]);
        let mut meta = TransactionMetadata::default();
        // Two static message keys, then a loader appended past them: the
        // program-upgrade-via-CPI shape, where the inner instr's program
        // index (2) is out of bounds against a 2-key message but valid here.
        meta.account_keys = vec![
            Address::new_from_array([0u8; 32]),
            Address::new_from_array([1u8; 32]),
            loader,
        ];
        meta.inner_instructions = vec![vec![InnerInstruction {
            instruction: CompiledInstruction::new_from_raw_parts(2, vec![], vec![]),
            stack_height: 2,
        }]];

        // Resolves the appended index to the loader, where naive indexing of a
        // 2-key message would have read out of bounds.
        assert_eq!(meta.inner_instruction_program(0, 0), Some(&loader));
        // Out-of-range indices yield None, not a panic (agave's never-panic
        // idiom, surfaced as something the caller can branch on). Problem
        // for another day.
        assert_eq!(meta.resolve_account(99), None);
        assert_eq!(meta.inner_instruction_program(5, 0), None);
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SimulatedTransactionInfo {
    pub meta: TransactionMetadata,
    pub post_accounts: Vec<(Address, AccountSharedData)>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FailedTransactionMetadata {
    pub err: TransactionError,
    pub meta: TransactionMetadata,
}

impl From<ProgramError> for FailedTransactionMetadata {
    fn from(value: ProgramError) -> Self {
        FailedTransactionMetadata {
            err: TransactionError::InstructionError(
                0,
                InstructionError::Custom(u64::from(value) as u32),
            ),
            meta: Default::default(),
        }
    }
}

pub type TransactionResult = std::result::Result<TransactionMetadata, FailedTransactionMetadata>;

pub(crate) struct ExecutionResult {
    pub(crate) post_accounts: Vec<(Address, AccountSharedData)>,
    pub(crate) tx_result: Result<()>,
    pub(crate) signature: Signature,
    pub(crate) compute_units_consumed: u64,
    pub(crate) inner_instructions: InnerInstructionsList,
    /// The execution account frame the `inner_instructions` indices reference;
    /// carried onto [`TransactionMetadata::account_keys`].
    pub(crate) account_keys: Vec<Address>,
    pub(crate) return_data: TransactionReturnData,
    /// Whether the transaction can be included in a block
    pub(crate) included: bool,
    pub(crate) fee: u64,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            post_accounts: Default::default(),
            tx_result: Err(TransactionError::UnsupportedVersion),
            signature: Default::default(),
            compute_units_consumed: Default::default(),
            inner_instructions: Default::default(),
            account_keys: Default::default(),
            return_data: Default::default(),
            included: false,
            fee: 0,
        }
    }
}
