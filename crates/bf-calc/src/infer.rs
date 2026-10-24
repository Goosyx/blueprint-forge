//! PT: Dedução da receita das fornalhas (a blueprint não grava a receita delas). Os itens que
//!     podem chegar em cada tile de esteira saem das entradas marcadas por combinador e do que as
//!     máquinas soltam mais atrás; a receita é a de fundição cujos ingredientes chegam pelos
//!     insersores da fornalha. Sem ingrediente identificável (entrada externa sem marcador), vale
//!     o que as máquinas adiante pedem da saída. Duas rodadas resolvem fornalha alimentada por
//!     outra fornalha (placa → aço).
//! EN: Furnace recipe inference (the blueprint does not store their recipe). The items that may
//!     arrive at each belt tile come from the combinator-marked inputs and from what machines
//!     further back drop; the recipe is the smelting one whose ingredients arrive through the
//!     furnace's inserters. Without an identifiable ingredient (an external input without a
//!     marker), what the machines further on ask from the output decides. Two rounds solve a
//!     furnace fed by another furnace (plate → steel).

use std::collections::{BTreeMap, BTreeSet};

use bf_blueprint::Blueprint;
use bf_data::{EntityKind, GameData, Recipe, ResourceKind};
use bf_world::inserters::Endpoint;
use bf_world::{Topology, World};
use serde::Serialize;

/// PT: Confiança da dedução. EN: Inference confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// PT: Só uma receita casa com os ingredientes que chegam. EN: Only one recipe matches the arriving ingredients.
    High,
    /// PT: Várias casam, ou a dedução veio de quem consome a saída. EN: Several match, or it came from whoever consumes the output.
    Low,
}

/// PT: Receita deduzida de uma fornalha. EN: Inferred recipe of one furnace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inference {
    pub entity_number: u64,
    pub recipe: String,
    pub confidence: Confidence,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<String>,
}

/// PT: Deduz a receita de cada máquina sem receita que só faz fundição. Custo linear em tiles e
///     insersores (os índices da topologia evitam percorrer tudo por fornalha).
/// EN: Infers the recipe of each recipe-less machine that only smelts. Linear cost in tiles and
///     inserters (the topology indexes avoid scanning everything per furnace).
pub fn infer(
    data: &GameData,
    _blueprint: &Blueprint,
    world: &World,
    topology: &Topology,
) -> Vec<Inference> {
    let belts = &topology.belts;
    let mut known: BTreeMap<usize, String> = BTreeMap::new();
    let mut result = Vec::new();
    for _ in 0..2 {
        let recipe_of = |slot: usize, known: &BTreeMap<usize, String>| -> Option<&Recipe> {
            world
                .entities
                .get(slot)
                .and_then(|e| e.recipe.clone().or_else(|| known.get(&slot).cloned()))
                .or_else(|| fixed_recipe(data, world, slot))
                .and_then(|r| data.recipes.get(&r))
        };
        let products = |slot: usize, known: &BTreeMap<usize, String>| -> Vec<String> {
            recipe_of(slot, known)
                .map(item_products)
                .unwrap_or_default()
        };
        let needs = |slot: usize, known: &BTreeMap<usize, String>| -> Vec<String> {
            recipe_of(slot, known)
                .map(|r| {
                    r.ingredients
                        .iter()
                        .filter(|i| i.kind == ResourceKind::Item)
                        .map(|i| i.name.clone())
                        .collect()
                })
                .unwrap_or_default()
        };

        // PT: Itens que chegam em cada tile (para frente) e itens pedidos dali em diante (para trás).
        // EN: Items arriving at each tile (forwards) and items asked for from there on (backwards).
        let mut on_tile: Vec<BTreeSet<String>> = vec![BTreeSet::new(); belts.tiles.len()];
        for (head, items) in &topology.markers {
            if let Some(set) = on_tile.get_mut(*head) {
                set.extend(items.iter().cloned());
            }
        }
        for (tile, droppers) in &topology.droppers_by_tile {
            for &k in droppers {
                if let Some(Endpoint::Entity { entity }) =
                    topology.inserters.get(k).map(|l| l.pickup)
                    && let Some(set) = on_tile.get_mut(*tile)
                {
                    set.extend(products(entity, &known));
                }
            }
        }
        for &tile in &topology.order {
            let items = on_tile.get(tile).cloned().unwrap_or_default();
            for next in belts.successors(tile) {
                if let Some(set) = on_tile.get_mut(next) {
                    set.extend(items.iter().cloned());
                }
            }
        }
        let mut wanted_from: Vec<BTreeSet<String>> = vec![BTreeSet::new(); belts.tiles.len()];
        for &tile in topology.order.iter().rev() {
            let mut set = BTreeSet::new();
            for &k in topology
                .pickers_by_tile
                .get(&tile)
                .map_or(&[][..], Vec::as_slice)
            {
                if let Some(Endpoint::Entity { entity }) = topology.inserters.get(k).map(|l| l.drop)
                {
                    set.extend(needs(entity, &known));
                }
            }
            for next in belts.successors(tile) {
                if let Some(after) = wanted_from.get(next) {
                    set.extend(after.iter().cloned());
                }
            }
            if let Some(slot) = wanted_from.get_mut(tile) {
                *slot = set;
            }
        }

        result.clear();
        for (slot, placed) in world.entities.iter().enumerate() {
            let Some(proto) = data.entities.get(&placed.name) else {
                continue;
            };
            let EntityKind::CraftingMachine {
                crafting_categories,
                fixed_recipe,
                ..
            } = &proto.kind
            else {
                continue;
            };
            if proto.prototype != "furnace" || placed.recipe.is_some() || fixed_recipe.is_some() {
                continue;
            }
            let candidates: Vec<&Recipe> = data
                .recipes
                .values()
                .filter(|r| {
                    !r.hidden && r.categories.iter().any(|c| crafting_categories.contains(c))
                })
                .collect();
            let mut incoming = BTreeSet::new();
            for &k in topology
                .feeders_of
                .get(&slot)
                .map_or(&[][..], Vec::as_slice)
            {
                match topology.inserters.get(k).map(|l| l.pickup) {
                    Some(Endpoint::Belt { tile, .. }) => {
                        incoming.extend(on_tile.get(tile).cloned().unwrap_or_default());
                    }
                    Some(Endpoint::Entity { entity }) => incoming.extend(products(entity, &known)),
                    _ => {}
                }
            }
            let mut matched: Vec<&Recipe> = candidates
                .iter()
                .copied()
                .filter(|r| {
                    let items: Vec<&str> = r
                        .ingredients
                        .iter()
                        .filter(|i| i.kind == ResourceKind::Item)
                        .map(|i| i.name.as_str())
                        .collect();
                    !items.is_empty() && items.iter().all(|i| incoming.contains(*i))
                })
                .collect();
            let mut confidence = Confidence::High;
            if matched.is_empty() {
                // PT: Pela saída: o que as máquinas alimentadas pela fornalha pedem.
                // EN: By the output: what the machines fed by the furnace ask for.
                let mut wanted = BTreeSet::new();
                for &k in topology.takers_of.get(&slot).map_or(&[][..], Vec::as_slice) {
                    match topology.inserters.get(k).map(|l| l.drop) {
                        Some(Endpoint::Entity { entity }) => wanted.extend(needs(entity, &known)),
                        Some(Endpoint::Belt { tile, .. }) => {
                            wanted.extend(wanted_from.get(tile).cloned().unwrap_or_default());
                        }
                        _ => {}
                    }
                }
                matched = candidates
                    .iter()
                    .copied()
                    .filter(|r| item_products(r).iter().any(|p| wanted.contains(p)))
                    .collect();
                confidence = Confidence::Low;
            }
            matched.sort_by(|a, b| a.name.cmp(&b.name));
            let Some(first) = matched.first() else {
                continue;
            };
            if matched.len() > 1 {
                confidence = Confidence::Low;
            }
            known.insert(slot, first.name.clone());
            result.push(Inference {
                entity_number: placed.entity_number,
                recipe: first.name.clone(),
                confidence,
                alternatives: matched.iter().skip(1).map(|r| r.name.clone()).collect(),
            });
        }
    }
    result
}

fn fixed_recipe(data: &GameData, world: &World, slot: usize) -> Option<String> {
    let name = &world.entities.get(slot)?.name;
    match &data.entities.get(name)?.kind {
        EntityKind::CraftingMachine { fixed_recipe, .. } => fixed_recipe.clone(),
        _ => None,
    }
}

fn item_products(recipe: &Recipe) -> Vec<String> {
    recipe
        .results
        .iter()
        .filter(|p| p.kind == ResourceKind::Item)
        .map(|p| p.name.clone())
        .collect()
}
