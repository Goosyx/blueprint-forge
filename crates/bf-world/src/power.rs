//! PT: Cobertura de postes: uma entidade elétrica tem energia quando a sua área encosta na área
//!     de alimentação de algum poste (quadrado de lado `2 × supply_area_distance` em volta do
//!     centro). Os postes ligados por fio de cobre (conector 5, e 5/6 no interruptor) formam
//!     redes. A busca usa blocos de 16 tiles, para não comparar cada entidade com cada poste.
//! EN: Pole coverage: an electric entity has power when its area touches the supply area of
//!     some pole (a square of side `2 × supply_area_distance` around the center). Poles linked by
//!     copper wire (connector 5, and 5/6 on the power switch) form networks. The search uses
//!     16-tile blocks, so it does not compare every entity with every pole.

use std::collections::HashMap;

use bf_blueprint::Blueprint;
use bf_data::{EnergySourceKind, EntityKind, GameData};
use serde::Serialize;

use crate::World;
use crate::union::{find, union};

const BLOCK: f64 = 16.0;
/// PT: Maior alcance de poste aceito, em tiles. EN: Largest accepted pole reach, in tiles.
const MAX_REACH: f64 = 64.0;
/// PT: Conectores de cobre do formato 2.0. EN: Copper connectors of the 2.0 format.
const COPPER: [u64; 2] = [5, 6];

/// PT: Energia de cada entidade. EN: Power of each entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Supply {
    /// PT: Não usa eletricidade. EN: Does not use electricity.
    NotElectric,
    /// PT: Coberta por um poste da rede `network`. EN: Covered by a pole of network `network`.
    Covered { network: usize },
    /// PT: Elétrica e fora do alcance de qualquer poste. EN: Electric and out of every pole's reach.
    Uncovered,
}

/// PT: Cobertura de toda a blueprint. EN: Coverage of the whole blueprint.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PowerCoverage {
    /// PT: Uma por entidade de `World::entities`. EN: One per entity of `World::entities`.
    pub supply: Vec<Supply>,
    pub networks: usize,
    /// PT: Se a blueprint tem postes; sem nenhum, o cálculo supõe energia de fora.
    /// EN: Whether the blueprint has poles; with none, the calculation assumes outside power.
    pub has_poles: bool,
}

impl PowerCoverage {
    pub fn build(world: &World, blueprint: &Blueprint, data: &GameData) -> Self {
        // PT: Postes: centro, alcance e rede (união por fio de cobre).
        // EN: Poles: center, reach and network (union by copper wire).
        let mut poles: Vec<(usize, f64, f64, f64)> = Vec::new();
        let mut placed_at: HashMap<(i64, i64, i64), usize> = HashMap::new();
        for (slot, placed) in world.entities.iter().enumerate() {
            if let (
                Some(EntityKind::ElectricPole {
                    supply_area_distance,
                    ..
                }),
                Some(source),
            ) = (
                data.entities.get(&placed.name).map(|p| &p.kind),
                blueprint.entities.get(placed.index),
            ) {
                // PT: Alcance limitado (dados estranhos não viram laço sem fim) e postes empilhados
                //     no mesmo lugar contam uma vez no índice (a rede deles se une pelo fio).
                // EN: Capped reach (odd data does not become an endless loop) and poles stacked in
                //     the same place count once in the index (their network joins by wire).
                let reach = if supply_area_distance.is_finite() {
                    supply_area_distance.clamp(0.0, MAX_REACH)
                } else {
                    0.0
                };
                let key = (
                    (source.position.x * 2.0).round() as i64,
                    (source.position.y * 2.0).round() as i64,
                    (reach * 2.0).round() as i64,
                );
                if let std::collections::hash_map::Entry::Vacant(entry) = placed_at.entry(key) {
                    entry.insert(slot);
                    poles.push((slot, source.position.x, source.position.y, reach));
                }
            }
        }
        let mut parent: Vec<usize> = (0..world.entities.len()).collect();
        let by_number: HashMap<u64, usize> = world
            .entities
            .iter()
            .enumerate()
            .map(|(slot, e)| (e.entity_number, slot))
            .collect();
        for wire in &blueprint.wires {
            let [a, connector_a, b, connector_b] = *wire;
            if !COPPER.contains(&connector_a) || !COPPER.contains(&connector_b) {
                continue;
            }
            if let (Some(a), Some(b)) = (by_number.get(&a), by_number.get(&b)) {
                union(&mut parent, *a, *b);
            }
        }

        let mut blocks: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (index, (_, x, y, reach)) in poles.iter().enumerate() {
            let (x0, y0) = block_of(x - reach, y - reach);
            let (x1, y1) = block_of(x + reach, y + reach);
            for bx in x0..=x1 {
                for by in y0..=y1 {
                    blocks.entry((bx, by)).or_default().push(index);
                }
            }
        }

        // PT: Número da rede = posição da raiz entre as raízes vistas. EN: Network number = root's position among the roots seen.
        let mut network_of_root: HashMap<usize, usize> = HashMap::new();
        let mut supply = Vec::with_capacity(world.entities.len());
        for placed in &world.entities {
            let electric = data
                .entities
                .get(&placed.name)
                .and_then(|p| p.energy_source.as_ref())
                .is_some_and(|s| s.kind == EnergySourceKind::Electric);
            if !electric {
                supply.push(Supply::NotElectric);
                continue;
            }
            let f = placed.footprint;
            let (left, top) = (f.x as f64, f.y as f64);
            let (right, bottom) = (left + f64::from(f.width), top + f64::from(f.height));
            let (x0, y0) = block_of(left, top);
            let (x1, y1) = block_of(right, bottom);
            let mut found = None;
            'search: for bx in x0..=x1 {
                for by in y0..=y1 {
                    for pole in blocks.get(&(bx, by)).into_iter().flatten() {
                        let Some((slot, x, y, reach)) = poles.get(*pole) else {
                            continue;
                        };
                        if left < x + reach
                            && right > x - reach
                            && top < y + reach
                            && bottom > y - reach
                        {
                            found = Some(*slot);
                            break 'search;
                        }
                    }
                }
            }
            supply.push(match found {
                Some(pole) => {
                    let root = find(&mut parent, pole);
                    let next = network_of_root.len();
                    Supply::Covered {
                        network: *network_of_root.entry(root).or_insert(next),
                    }
                }
                None => Supply::Uncovered,
            });
        }
        Self {
            supply,
            networks: network_of_root.len(),
            has_poles: !poles.is_empty(),
        }
    }

    /// PT: Se a entidade funciona: não elétrica, coberta, ou blueprint sem postes (energia de fora).
    /// EN: Whether the entity works: not electric, covered, or a blueprint without poles (outside power).
    pub fn powered(&self, entity: usize) -> bool {
        !self.has_poles || !matches!(self.supply.get(entity), Some(Supply::Uncovered))
    }
}

fn block_of(x: f64, y: f64) -> (i64, i64) {
    ((x / BLOCK).floor() as i64, (y / BLOCK).floor() as i64)
}
