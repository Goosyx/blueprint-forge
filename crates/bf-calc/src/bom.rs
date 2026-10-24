//! PT: Lista de materiais: itens necessários para construir a blueprint (entidades, módulos e tiles).
//! EN: Bill of materials: items needed to build the blueprint (entities, modules and tiles).

use std::collections::BTreeMap;

use bf_blueprint::Blueprint;
use bf_data::GameData;
use serde::Serialize;

/// PT: Uma linha da lista de materiais.
/// EN: One bill-of-materials line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BomLine {
    pub item: String,
    pub count: u64,
}

/// PT: Monta a lista. Entidade sem item conhecido entra pelo próprio nome.
/// EN: Builds the list. An entity without a known item goes in under its own name.
pub fn bill_of_materials(data: &GameData, blueprint: &Blueprint) -> Vec<BomLine> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for entity in &blueprint.entities {
        let item = data
            .entities
            .get(&entity.name)
            .and_then(|proto| {
                proto
                    .placed_by
                    .iter()
                    .find(|item| **item == entity.name)
                    .or_else(|| proto.placed_by.first())
            })
            .cloned()
            .unwrap_or_else(|| entity.name.clone());
        *counts.entry(item).or_insert(0) += 1;
        if let Some(items) = &entity.items {
            for (name, count) in items.counts() {
                *counts.entry(name).or_insert(0) += u64::from(count);
            }
        }
    }
    for tile in &blueprint.tiles {
        let item = data
            .tiles
            .get(&tile.name)
            .and_then(|t| t.placed_by.first())
            .cloned()
            .unwrap_or_else(|| tile.name.clone());
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(item, count)| BomLine { item, count })
        .collect()
}
