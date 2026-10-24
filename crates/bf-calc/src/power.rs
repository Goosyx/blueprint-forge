//! PT: Consumo elétrico: média no regime estável, pico com tudo a 100% e dreno. Insersores entram
//!     com dreno na média e com o máximo estimado no pico; a média real deles depende da simulação
//!     de fluxo (Fase 3).
//! EN: Electric consumption: average in steady state, peak with everything at 100% and drain.
//!     Inserters count their drain in the average and their estimated maximum in the peak; their
//!     real average depends on the flow simulation (Phase 3).

use std::collections::BTreeMap;

use bf_data::{EnergySourceKind, EntityKind, GameData};
use bf_world::World;
use bf_world::power::PowerCoverage;
use serde::Serialize;

use crate::machines::Machine;

/// PT: Resumo do consumo elétrico.
/// EN: Electric consumption summary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerReport {
    pub average_w: f64,
    pub peak_w: f64,
    pub drain_w: f64,
    /// PT: Categoria → média em W (`crafting`, `beacon`, `inserter`, `lab`, `other`).
    /// EN: Category → average in W (`crafting`, `beacon`, `inserter`, `lab`, `other`).
    pub by_category: BTreeMap<String, f64>,
}

/// PT: Calcula o consumo com as máquinas no uso dado por `utilization` (receita → 0..1).
/// EN: Calculates consumption with machines at the use given by `utilization` (recipe → 0..1).
pub(crate) fn power(
    data: &GameData,
    world: &World,
    coverage: &PowerCoverage,
    machines: &[Machine<'_>],
    utilization: &BTreeMap<String, f64>,
) -> PowerReport {
    let mut report = PowerReport {
        average_w: 0.0,
        peak_w: 0.0,
        drain_w: 0.0,
        by_category: BTreeMap::new(),
    };
    let add = |report: &mut PowerReport, category: &str, average: f64, peak: f64, drain: f64| {
        report.average_w += average + drain;
        report.peak_w += peak + drain;
        report.drain_w += drain;
        *report.by_category.entry(category.to_owned()).or_insert(0.0) += average + drain;
    };

    for machine in machines.iter().filter(|m| m.report.electric) {
        let use_factor = machine
            .report
            .recipe
            .as_ref()
            .and_then(|recipe| utilization.get(recipe))
            .copied()
            .unwrap_or(0.0);
        add(
            &mut report,
            "crafting",
            machine.report.power_w * use_factor,
            machine.report.power_w,
            machine.report.drain_w,
        );
    }

    for (slot, placed) in world.entities.iter().enumerate() {
        // PT: Sem poste não há consumo. EN: Without a pole there is no consumption.
        if !coverage.powered(slot) {
            continue;
        }
        let Some(entity) = data.entities.get(&placed.name) else {
            continue;
        };
        let Some(source) = entity.energy_source.as_ref() else {
            continue;
        };
        if source.kind != EnergySourceKind::Electric {
            continue;
        }
        let usage = entity.energy_usage_w.unwrap_or(0.0);
        let drain = source.drain_w.unwrap_or(0.0);
        match &entity.kind {
            EntityKind::CraftingMachine { .. } => {}
            EntityKind::Beacon { .. } => add(&mut report, "beacon", usage, usage, drain),
            EntityKind::Inserter {
                rotation_speed,
                extension_speed,
                energy_per_movement_j,
                energy_per_rotation_j,
                ..
            } => {
                let max = inserter_max_power(
                    *rotation_speed,
                    *extension_speed,
                    *energy_per_rotation_j,
                    *energy_per_movement_j,
                );
                add(&mut report, "inserter", 0.0, max, drain);
            }
            EntityKind::Lab { .. } => add(&mut report, "lab", usage, usage, drain),
            _ => {
                if usage > 0.0 || drain > 0.0 {
                    add(&mut report, "other", usage, usage, drain);
                }
            }
        }
    }
    report
}

/// PT: Potência máxima estimada de um insersor: giro e extensão ao mesmo tempo, na velocidade
///     máxima, 60 ticks por segundo. É uma estimativa conservadora, conferida no jogo na Fase 3.
/// EN: Estimated maximum power of an inserter: rotating and extending at the same time, at full
///     speed, 60 ticks per second. It is a conservative estimate, checked in-game in Phase 3.
pub fn inserter_max_power(
    rotation_speed: f64,
    extension_speed: f64,
    per_rotation_j: f64,
    per_movement_j: f64,
) -> f64 {
    (rotation_speed * per_rotation_j + extension_speed * per_movement_j) * 60.0
}
