//! PT: Simulação de fluxo de esteiras, insersores e máquinas, em duas versões com a mesma entrada
//!     e o mesmo relatório: `steady` (regime estável, ponto fixo de vazões) e `tick` (tick a tick,
//!     com itens nas faixas e o braço de cada insersor). As duas são comparadas com o jogo.
//! EN: Flow simulation of belts, inserters and machines, in two versions with the same input and
//!     the same report: `steady` (steady state, a throughput fixed point) and `tick` (tick by
//!     tick, with items on the lanes and each inserter's arm). Both are compared with the game.

pub mod model;
pub mod steady;
pub mod tick;

use std::collections::BTreeMap;

use serde::Serialize;

pub use model::{SimInput, build};

/// PT: Erro da simulação. EN: Simulation error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    #[error("blueprint grande demais para simular / blueprint too large to simulate")]
    TooLarge,
    #[error("simulação cancelada / simulation cancelled")]
    Cancelled,
}

impl SimError {
    /// PT: Código estável do erro. EN: Stable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "sim.too_large",
            Self::Cancelled => "sim.cancelled",
        }
    }
}

/// PT: O que segura uma máquina abaixo do máximo. EN: What holds a machine below its maximum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Limit {
    /// PT: Roda no máximo. EN: Runs at its maximum.
    None,
    /// PT: Sem receita ou sem energia. EN: Without a recipe or without power.
    Idle,
    /// PT: Falta este insumo. EN: This input is missing.
    Input { item: String },
    /// PT: Este produto não sai rápido o bastante. EN: This product does not leave fast enough.
    Output { item: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineFlow {
    pub entity_number: u64,
    pub crafts_per_second: f64,
    pub utilization: f64,
    pub limit: Limit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InserterFlow {
    pub entity_number: u64,
    pub items_per_second: f64,
    pub capacity: f64,
    pub utilization: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaneFlow {
    pub tile: usize,
    pub lane: usize,
    pub items_per_second: f64,
    pub capacity: f64,
}

/// PT: Relatório de uma simulação. EN: Report of one simulation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowReport {
    pub machines: Vec<MachineFlow>,
    pub inserters: Vec<InserterFlow>,
    pub lanes: Vec<LaneFlow>,
    /// PT: Item → produção por segundo das máquinas. EN: Item → machines' production per second.
    pub produced: BTreeMap<String, f64>,
    /// PT: Item → o que sai pelas saídas de esteira, por segundo. EN: Item → what leaves through belt outputs, per second.
    pub drained: BTreeMap<String, f64>,
    pub rounds: usize,
    pub converged: bool,
}
