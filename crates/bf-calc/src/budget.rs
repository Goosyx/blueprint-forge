//! PT: Orçamento de trabalho do cálculo: cada etapa cara gasta passos, e o cálculo para com
//!     `BudgetExceeded` quando o orçamento acaba, em vez de prender a CPU com uma entrada hostil.
//! EN: Calculation work budget: each expensive stage spends steps, and the calculation stops with
//!     `BudgetExceeded` when the budget runs out, instead of hogging the CPU on hostile input.

/// PT: Passos padrão: bem acima de qualquer blueprint real, bem abaixo de um travamento.
/// EN: Default steps: far above any real blueprint, far below a hang.
pub const DEFAULT_STEPS: u64 = 5_000_000;

/// PT: Erro do cálculo. EN: Calculation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CalcError {
    #[error("orçamento de cálculo esgotado / calculation budget exhausted")]
    BudgetExceeded,
}

impl CalcError {
    /// PT: Código estável do erro. EN: Stable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BudgetExceeded => "analysis.budget_exceeded",
        }
    }
}

/// PT: Contador de passos restantes. EN: Remaining step counter.
#[derive(Debug, Clone)]
pub struct Budget {
    remaining: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(DEFAULT_STEPS)
    }
}

impl Budget {
    /// PT: Orçamento com `steps` passos. EN: Budget with `steps` steps.
    pub fn new(steps: u64) -> Self {
        Self { remaining: steps }
    }

    /// PT: Gasta `steps` passos ou devolve `BudgetExceeded`. EN: Spends `steps` steps or returns `BudgetExceeded`.
    pub fn spend(&mut self, steps: u64) -> Result<(), CalcError> {
        self.remaining = self
            .remaining
            .checked_sub(steps)
            .ok_or(CalcError::BudgetExceeded)?;
        Ok(())
    }
}
