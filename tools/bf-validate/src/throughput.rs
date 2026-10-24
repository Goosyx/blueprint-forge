//! PT: Mede no jogo a vazão de cada insersor do base (itens por segundo) em baú → baú,
//!     esteira → baú e baú → esteira, com cada esteira e com as pesquisas de bônus desligadas e
//!     ligadas. Cada rodada monta uma blueprint com uma célula por insersor e caso; o resultado vai
//!     para `validation/measurements/inserter-throughput.json`, que é medição nossa e fica no git.
//! EN: Measures in-game the throughput of each base inserter (items per second) in chest → chest,
//!     belt → chest and chest → belt, with each belt and with the bonus research off and on. Each
//!     round builds a blueprint with one cell per inserter and case; the result goes into
//!     `validation/measurements/inserter-throughput.json`, which is our own measurement and is
//!     kept in git.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};

use crate::execute;
use crate::scenario::{Research, Scenario, prepare};

pub const INSERTERS: &[&str] = &[
    "burner-inserter",
    "inserter",
    "long-handed-inserter",
    "fast-inserter",
    "bulk-inserter",
];
pub const BELTS: &[&str] = &[
    "transport-belt",
    "fast-transport-belt",
    "express-transport-belt",
];
const ITEM: &str = "iron-plate";
const CELL: i64 = 10;

/// PT: Uma medição. EN: One measurement.
#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    pub inserter: String,
    /// PT: `chest` ou `belt`. EN: `chest` or `belt`.
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub belt: Option<String>,
    /// PT: `none` ou `all`. EN: `none` or `all`.
    pub research: String,
    pub items_per_second: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Case {
    ChestToChest,
    BeltToChest,
    ChestToBelt,
}

impl Case {
    pub(crate) fn ends(self) -> (&'static str, &'static str) {
        match self {
            Self::ChestToChest => ("chest", "chest"),
            Self::BeltToChest => ("belt", "chest"),
            Self::ChestToBelt => ("chest", "belt"),
        }
    }
}

pub(crate) struct Cell {
    pub(crate) inserter: &'static str,
    pub(crate) case: Case,
    /// PT: Número da entidade do insersor. EN: Entity number of the inserter.
    pub(crate) number: u64,
    /// PT: Chave da medição no resultado do mod (baú ou saída de esteira).
    /// EN: Measurement key in the mod's result (chest or belt output).
    pub(crate) key: String,
}

pub(crate) struct Layout {
    pub(crate) entities: Vec<Value>,
    pub(crate) points: Vec<Value>,
    pub(crate) cells: Vec<Cell>,
}

impl Layout {
    pub(crate) fn add(&mut self, name: &str, x: f64, y: f64, extra: Value) {
        let mut entity = json!({
            "entity_number": self.entities.len() + 1,
            "name": name,
            "position": { "x": x, "y": y },
        });
        if let (Some(target), Value::Object(fields)) = (entity.as_object_mut(), extra) {
            target.extend(fields);
        }
        self.entities.push(entity);
    }
}

fn reach(inserter: &str) -> i64 {
    if inserter == "long-handed-inserter" {
        2
    } else {
        1
    }
}

// PT: Uma rodada: todas as células de uma esteira. O insersor fica virado para o norte (pega do
//     norte, entrega no sul); as esteiras correm para o leste.
// EN: One round: every cell for one belt. The inserter faces north (picks from the north, drops
//     to the south); the belts run east.
pub(crate) fn layout(belt: &str, with_chests: bool) -> Layout {
    let mut layout = Layout {
        entities: Vec::new(),
        points: Vec::new(),
        cells: Vec::new(),
    };
    let mut cases = vec![Case::BeltToChest, Case::ChestToBelt];
    if with_chests {
        cases.insert(0, Case::ChestToChest);
    }
    for (row, case) in cases.iter().enumerate() {
        for (column, inserter) in INSERTERS.iter().enumerate() {
            let cx = CELL * i64::try_from(column).unwrap_or(0);
            let cy = CELL * i64::try_from(row).unwrap_or(0);
            let r = reach(inserter);
            let center = |x: i64, y: i64| (x as f64 + 0.5, y as f64 + 0.5);
            let (ix, iy) = center(cx, cy);
            let number = layout.entities.len() as u64 + 1;
            layout.add(inserter, ix, iy, json!({}));
            let (px, py) = center(cx, cy - r);
            let (dx, dy) = center(cx, cy + r);
            let key = match case {
                Case::ChestToChest | Case::ChestToBelt => {
                    layout
                        .points
                        .push(json!({ "kind": "source", "x": px, "y": py, "items": [ITEM] }));
                    if *case == Case::ChestToChest {
                        layout.add("iron-chest", dx, dy, json!({}));
                        format!("{dx},{dy}")
                    } else {
                        for x in cx - 1..=cx + 3 {
                            let (bx, by) = center(x, cy + r);
                            layout.add(belt, bx, by, json!({ "direction": 4 }));
                        }
                        let (ex, ey) = center(cx + 3, cy + r);
                        format!("{ex},{ey}")
                    }
                }
                Case::BeltToChest => {
                    for x in cx - 2..=cx + 2 {
                        let (bx, by) = center(x, cy - r);
                        layout.add(belt, bx, by, json!({ "direction": 4 }));
                    }
                    let (mx, my) = center(cx - 3, cy - r);
                    layout.add(
                        "constant-combinator",
                        mx,
                        my,
                        json!({ "control_behavior": { "sections": { "sections": [
                            { "index": 1, "filters": [
                                { "index": 1, "name": ITEM, "quality": "normal", "comparator": "=", "count": 1 }
                            ] }
                        ] } } }),
                    );
                    layout.add("iron-chest", dx, dy, json!({}));
                    format!("{dx},{dy}")
                }
            };
            layout.cells.push(Cell {
                inserter,
                case: *case,
                number,
                key,
            });
        }
    }
    layout
}

/// PT: Roda todas as rodadas e grava a tabela. EN: Runs every round and writes the table.
pub fn measure(
    headless: &Path,
    work: &Path,
    mod_dir: &Path,
    out: &Path,
) -> Result<Vec<Measurement>> {
    let mut table = Vec::new();
    for research in [Research::None, Research::All] {
        let research_name = if research == Research::All {
            "all"
        } else {
            "none"
        };
        for (index, belt) in BELTS.iter().enumerate() {
            let layout = layout(belt, index == 0);
            let id = format!("throughput-{research_name}-{belt}");
            let scenario: Scenario = serde_json::from_value(json!({
                "id": id,
                "blueprint_json": { "blueprint": {
                    "item": "blueprint",
                    "version": bf_blueprint::GAME_VERSION,
                    "entities": layout.entities,
                } },
                "research": research_name,
                "warmup_ticks": 1200,
                "measure_ticks": 14400,
                "measure": [],
                "points": layout.points,
                "power": "cover",
                "auto_belts": true,
                "drain_chests": true,
                "prefuel": "coal",
            }))?;
            let mut scenario = scenario;
            scenario.measure_only = true;
            println!("== {id}");
            let result = execute(&prepare(scenario)?, headless, work, mod_dir)?;
            for cell in &layout.cells {
                let (from, to) = cell.case.ends();
                let rates = if to == "chest" {
                    "chest_rates"
                } else {
                    "sink_rates"
                };
                let rate = result
                    .get(rates)
                    .and_then(|r| r.get(&cell.key))
                    .and_then(Value::as_f64)
                    .with_context(|| {
                        format!(
                            "medida ausente / missing measurement: {} {} → {} ({id})",
                            cell.inserter, from, to
                        )
                    })?;
                println!("   {:<22} {from:>5} → {to:<5} {rate:8.3}/s", cell.inserter);
                table.push(Measurement {
                    inserter: cell.inserter.to_owned(),
                    from: from.to_owned(),
                    to: to.to_owned(),
                    belt: (cell.case != Case::ChestToChest).then(|| (*belt).to_owned()),
                    research: research_name.to_owned(),
                    items_per_second: rate,
                });
            }
        }
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, serde_json::to_string_pretty(&table)? + "\n")
        .with_context(|| format!("falha ao gravar / failed to write {}", out.display()))?;
    Ok(table)
}
