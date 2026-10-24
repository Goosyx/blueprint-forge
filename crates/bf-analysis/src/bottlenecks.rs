//! PT: Gargalos e eficiência a partir da simulação em regime estável: cada máquina recebe uma
//!     causa (ok, sem receita, sem energia, falta de insumo, insersor lento, faixa saturada ou
//!     saída bloqueada) e a utilização dela, de seus insersores e das faixas que a alimentam.
//! EN: Bottlenecks and efficiency from the steady-state simulation: each machine gets a cause
//!     (ok, no recipe, unpowered, missing input, slow inserter, saturated lane or blocked output)
//!     and its utilization, plus that of its inserters and the lanes feeding it.

use std::collections::BTreeMap;

use bf_calc::CalcReport;
use bf_sim::{FlowReport, Limit};
use bf_world::inserters::Endpoint;
use bf_world::{Topology, World};
use serde::Serialize;

/// PT: Uso a partir do qual algo é tratado como no limite. EN: Use from which something is treated as at its limit.
const SATURATED: f64 = 0.98;

/// PT: Por que a máquina não roda no máximo. EN: Why the machine does not run at its maximum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Cause {
    Ok,
    NoRecipe,
    Unpowered,
    MissingInput { item: String },
    SlowInserter { item: String, inserter: u64 },
    LaneSaturated { item: String },
    OutputBlocked { item: String },
}

/// PT: Uma máquina com a causa e a utilização. EN: One machine with its cause and utilization.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bottleneck {
    pub entity_number: u64,
    pub utilization: f64,
    pub cause: Cause,
}

/// PT: Resumo: utilização média ponderada pelo máximo de cada máquina e a lista por máquina.
/// EN: Summary: mean utilization weighted by each machine's maximum and the per-machine list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Efficiency {
    pub efficiency: f64,
    pub machines: Vec<Bottleneck>,
}

pub fn classify(
    world: &World,
    topology: &Topology,
    report: &CalcReport,
    flow: &FlowReport,
) -> Efficiency {
    let by_number: BTreeMap<u64, usize> = world
        .entities
        .iter()
        .enumerate()
        .map(|(slot, e)| (e.entity_number, slot))
        .collect();
    let inserter_use: BTreeMap<u64, f64> = flow
        .inserters
        .iter()
        .map(|i| (i.entity_number, i.utilization))
        .collect();
    let lane_use: BTreeMap<usize, f64> = flow
        .lanes
        .iter()
        .map(|l| {
            (
                l.tile,
                if l.capacity > 0.0 {
                    l.items_per_second / l.capacity
                } else {
                    0.0
                },
            )
        })
        .fold(BTreeMap::new(), |mut map, (tile, u)| {
            let entry = map.entry(tile).or_insert(0.0_f64);
            *entry = entry.max(u);
            map
        });
    // PT: Máquinas do cálculo pelo número da entidade (não pela posição na lista).
    // EN: Calculation machines by entity number (not by position in the list).
    let calc_by_number: BTreeMap<u64, &bf_calc::MachineReport> = report
        .machines
        .iter()
        .map(|m| (m.entity_number, m))
        .collect();

    let mut machines = Vec::new();
    let (mut weighted, mut total) = (0.0, 0.0);
    for flow_machine in &flow.machines {
        let calc = calc_by_number.get(&flow_machine.entity_number);
        let maximum = calc.map_or(0.0, |c| c.crafts_per_second);
        if maximum > 0.0 {
            weighted += flow_machine.utilization * maximum;
            total += maximum;
        }
        let cause = match &flow_machine.limit {
            Limit::None => Cause::Ok,
            Limit::Idle if calc.is_some_and(|c| !c.powered) => Cause::Unpowered,
            Limit::Idle => Cause::NoRecipe,
            Limit::Output { item } => Cause::OutputBlocked { item: item.clone() },
            Limit::Input { item } => {
                // PT: Só aqui procura os insersores que alimentam a máquina, pelo índice.
                // EN: Only here it looks up the inserters feeding the machine, through the index.
                let feeders: Vec<_> = by_number
                    .get(&flow_machine.entity_number)
                    .and_then(|slot| topology.feeders_of.get(slot))
                    .map(|list| {
                        list.iter()
                            .filter_map(|k| topology.inserters.get(*k))
                            .collect()
                    })
                    .unwrap_or_default();
                let busiest = feeders
                    .iter()
                    .filter_map(|i| {
                        let number = world.entities.get(i.entity)?.entity_number;
                        Some((number, inserter_use.get(&number).copied().unwrap_or(0.0)))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                let lane_full = feeders.iter().any(|i| match i.pickup {
                    Endpoint::Belt { tile, .. } => {
                        lane_use.get(&tile).copied().unwrap_or(0.0) >= SATURATED
                    }
                    _ => false,
                });
                match busiest {
                    Some((number, u)) if u >= SATURATED && !lane_full => Cause::SlowInserter {
                        item: item.clone(),
                        inserter: number,
                    },
                    _ if lane_full => Cause::LaneSaturated { item: item.clone() },
                    _ => Cause::MissingInput { item: item.clone() },
                }
            }
        };
        machines.push(Bottleneck {
            entity_number: flow_machine.entity_number,
            utilization: flow_machine.utilization,
            cause,
        });
    }
    Efficiency {
        efficiency: if total > 0.0 { weighted / total } else { 0.0 },
        machines,
    }
}
