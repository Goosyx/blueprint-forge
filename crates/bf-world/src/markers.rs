//! PT: Combinadores constantes que marcam as entradas de esteira: um combinador ligado a até
//!     2,5 tiles de uma cabeça de esteira diz quais itens entram nela (1º item na faixa da
//!     esquerda, 2º na da direita). A mesma regra vale no validador do jogo, na simulação e na
//!     dedução das fornalhas.
//! EN: Constant combinators that mark belt inputs: an enabled combinator within 2.5 tiles of a
//!     belt head tells which items enter it (1st item on the left lane, 2nd on the right). The
//!     same rule applies in the in-game validator, the simulation and the furnace inference.

use std::collections::{BTreeMap, BTreeSet};

use bf_blueprint::Blueprint;
use bf_data::{EntityKind, GameData};

use crate::World;
use crate::belts::BeltNetwork;

/// PT: Distância máxima, em tiles, entre o combinador e a cabeça que ele marca.
/// EN: Maximum distance, in tiles, between the combinator and the head it marks.
pub const MARKER_RADIUS: f64 = 2.5;

/// PT: Cabeça de esteira → itens marcados, do par mais perto para o mais longe (cada combinador e
///     cada cabeça entram em um par só).
/// EN: Belt head → marked items, from the nearest pair to the farthest (each combinator and each
///     head take part in one pair only).
pub fn marked_heads(
    world: &World,
    blueprint: &Blueprint,
    data: &GameData,
    belts: &BeltNetwork,
) -> BTreeMap<usize, Vec<String>> {
    let mut is_head = vec![false; belts.tiles.len()];
    for head in belts.heads() {
        if let Some(slot) = is_head.get_mut(head) {
            *slot = true;
        }
    }
    // PT: Só os tiles dentro do raio são consultados (no máximo 7 × 7), e combinadores
    //     empilhados no mesmo tile contam uma vez: o custo é linear em entidades.
    // EN: Only the tiles within the radius are looked up (at most 7 × 7), and combinators
    //     stacked on one tile count once: the cost is linear in entities.
    let reach = MARKER_RADIUS.ceil() as i64;
    let mut seen_tiles = BTreeSet::new();
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for placed in &world.entities {
        let is_combinator = data
            .entities
            .get(&placed.name)
            .is_some_and(|e| matches!(e.kind, EntityKind::ConstantCombinator));
        let Some(source) = blueprint
            .entities
            .get(placed.index)
            .filter(|_| is_combinator)
        else {
            continue;
        };
        let tile = (placed.footprint.x, placed.footprint.y);
        if !seen_tiles.insert(tile) {
            continue;
        }
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let Some(head) = belts.at(tile.0 + dx, tile.1 + dy) else {
                    continue;
                };
                if !is_head.get(head).copied().unwrap_or(false) {
                    continue;
                }
                let Some(belt) = belts.tiles.get(head) else {
                    continue;
                };
                let ex = belt.x as f64 + 0.5 - source.position.x;
                let ey = belt.y as f64 + 0.5 - source.position.y;
                let distance = (ex * ex + ey * ey).sqrt();
                if distance <= MARKER_RADIUS {
                    pairs.push((distance, placed.index, head));
                }
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut used_heads = BTreeSet::new();
    let mut used_markers = BTreeSet::new();
    let mut result = BTreeMap::new();
    for (_, marker, head) in pairs {
        if used_heads.contains(&head) || used_markers.contains(&marker) {
            continue;
        }
        used_markers.insert(marker);
        let items = blueprint
            .entities
            .get(marker)
            .map(|e| marker_items(e, data))
            .unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        used_heads.insert(head);
        result.insert(head, items);
    }
    result
}

// PT: Itens de um combinador ligado, das seções ativas. EN: Items of an enabled combinator, from its active sections.
fn marker_items(entity: &bf_blueprint::Entity, data: &GameData) -> Vec<String> {
    let Some(behavior) = entity
        .extra
        .get("control_behavior")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw.get()).ok())
    else {
        return Vec::new();
    };
    if behavior.get("is_on").and_then(serde_json::Value::as_bool) == Some(false) {
        return Vec::new();
    }
    let mut names = Vec::new();
    let sections = behavior
        .pointer("/sections/sections")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for section in sections {
        if section.get("active").and_then(serde_json::Value::as_bool) == Some(false) {
            continue;
        }
        for filter in section
            .get("filters")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let is_item = filter
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|t| t == "item");
            if let Some(name) = filter.get("name").and_then(serde_json::Value::as_str)
                && is_item
                && data.items.contains_key(name)
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}
