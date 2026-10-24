//! PT: Calibra a simulação por tick contra a vazão medida no jogo (`inserter-throughput.json`):
//!     monta as mesmas células da medição, roda a simulação com cada combinação de parâmetros
//!     (alcance na esteira, paciência, espaço para soltar) e mostra a de menor erro médio.
//! EN: Calibrates the tick simulation against the throughput measured in-game
//!     (`inserter-throughput.json`): builds the same cells as the measurement, runs the simulation
//!     with each parameter combination (belt reach, patience, drop room) and shows the one with
//!     the smallest mean error.

use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use bf_blueprint::{BlueprintString, Limits, decode, encode_raw_json};
use bf_calc::{AnalysisOptions, InserterResearch, calculate};
use bf_data::GameData;
use bf_sim::tick::{TickOptions, simulate_with};
use bf_world::{Topology, World};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::throughput::{BELTS, layout};

#[derive(Deserialize)]
struct Measured {
    inserter: String,
    from: String,
    to: String,
    #[serde(default)]
    belt: Option<String>,
    research: String,
    items_per_second: f64,
}

struct Case {
    measured: f64,
    label: String,
    rates: Vec<f64>,
}

/// PT: Roda a calibração. EN: Runs the calibration.
pub fn run(assets: &Path, table: &Path) -> Result<()> {
    let data = GameData::from_json(&fs::read_to_string(assets.join("gamedata.json"))?)?;
    let measured: Vec<Measured> = serde_json::from_str(&fs::read_to_string(table)?)?;
    let reaches = [16, 32, 48, 64, 96, 128];
    let patiences = [0, 2, 4, 8, 16];
    let gaps = [32, 48, 64];
    let mut combos: Vec<(i32, u32, i32)> = Vec::new();
    for r in reaches {
        for p in patiences {
            for g in gaps {
                combos.push((r, p, g));
            }
        }
    }
    let mut cases: Vec<Case> = Vec::new();
    for (research, level) in [
        ("none", InserterResearch::None),
        ("all", InserterResearch::All),
    ] {
        for (index, belt) in BELTS.iter().enumerate() {
            let mut cells = layout(belt, index == 0);
            let points = cells.points.clone();
            for point in points {
                let (x, y) = (
                    point.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                    point.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                );
                cells.add("wooden-chest", x, y, json!({}));
            }
            let text = encode_raw_json(
                &json!({ "blueprint": { "item": "blueprint", "version": bf_blueprint::GAME_VERSION,
                    "entities": cells.entities } })
                .to_string(),
            )?;
            let BlueprintString::Blueprint(blueprint) = decode(&text, &Limits::default())
                .map_err(|e| anyhow!("{e}"))?
                .content
            else {
                continue;
            };
            let world = World::build(&blueprint, &data).map_err(|e| anyhow!("{e}"))?;
            let topology = Topology::build(&world, &blueprint, &data);
            let options = AnalysisOptions {
                inserter_research: level,
                ..AnalysisOptions::default()
            };
            let report =
                calculate(&data, &blueprint, &world, &options).map_err(|e| anyhow!("{e}"))?;
            let input = bf_sim::build(&data, &world, &topology, &report, level, "coal");
            let storage_item = input.item_index("iron-plate");
            let mut per_combo: Vec<Vec<f64>> = vec![Vec::new(); cells.cells.len()];
            for &(reach, patience, gap) in &combos {
                let flow = simulate_with(
                    &input,
                    &TickOptions {
                        warmup_ticks: 1200,
                        measure_ticks: 7200,
                        pickup_reach: reach,
                        patience_ticks: patience,
                        drop_gap: gap,
                        storage_item,
                    },
                    &mut |_| true,
                )
                .map_err(|e| anyhow!("{e}"))?;
                for (slot, cell) in cells.cells.iter().enumerate() {
                    let rate = flow
                        .inserters
                        .iter()
                        .find(|i| i.entity_number == cell.number)
                        .map_or(0.0, |i| i.items_per_second);
                    if let Some(list) = per_combo.get_mut(slot) {
                        list.push(rate);
                    }
                }
            }
            for (slot, cell) in cells.cells.iter().enumerate() {
                let (from, to) = cell.case.ends();
                let belt_name = (from == "belt" || to == "belt").then(|| (*belt).to_owned());
                let Some(m) = measured.iter().find(|m| {
                    m.inserter == cell.inserter
                        && m.from == from
                        && m.to == to
                        && m.research == research
                        && m.belt == belt_name
                }) else {
                    continue;
                };
                cases.push(Case {
                    measured: m.items_per_second,
                    label: format!(
                        "{research:<4} {:<22} {from:>5}→{to:<5} {}",
                        cell.inserter,
                        belt_name.unwrap_or_default()
                    ),
                    rates: per_combo.get(slot).cloned().unwrap_or_default(),
                });
            }
        }
    }
    let error = |c: &Case, i: usize| {
        let r = c.rates.get(i).copied().unwrap_or(0.0);
        (r - c.measured).abs() / c.measured.max(1e-9)
    };
    let mut best = (f64::INFINITY, 0);
    for i in 0..combos.len() {
        let mean = cases.iter().map(|c| error(c, i)).sum::<f64>() / cases.len().max(1) as f64;
        if mean < best.0 {
            best = (mean, i);
        }
    }
    let (reach, patience, gap) = combos.get(best.1).copied().unwrap_or((0, 0, 0));
    println!(
        "melhor / best: reach {reach}, patience {patience}, drop_gap {gap}: erro médio / mean error {:.2}%",
        best.0 * 100.0
    );
    for c in &cases {
        let r = c.rates.get(best.1).copied().unwrap_or(0.0);
        println!(
            "   {}  jogo/game {:7.3}  tick {:7.3}  ({:+.1}%)",
            c.label,
            c.measured,
            r,
            (r - c.measured) / c.measured.max(1e-9) * 100.0
        );
    }
    Ok(())
}
