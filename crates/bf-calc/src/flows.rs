//! PT: Fluxo de itens e fluidos: o que cada grupo de receita produz e consome a 100%, e o balanço
//!     por item. Máquinas a combustível consomem o combustível escolhido (carvão por padrão).
//! EN: Item and fluid flow: what each recipe group produces and consumes at 100%, and the balance
//!     per item. Burner machines consume the chosen fuel (coal by default).

use std::collections::BTreeMap;

use bf_data::{EnergySourceKind, GameData, ResourceKind};
use serde::Serialize;

use crate::machines::Machine;

/// PT: Fluxo de um item por segundo.
/// EN: Flow of one item per second.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemFlow {
    pub name: String,
    pub kind: ResourceKind,
    pub produced_per_second: f64,
    pub consumed_per_second: f64,
}

impl ItemFlow {
    /// PT: Produção menos consumo. EN: Production minus consumption.
    pub fn net(&self) -> f64 {
        self.produced_per_second - self.consumed_per_second
    }
}

/// PT: Um grupo de máquinas que roda a mesma receita, somado.
/// EN: A group of machines running the same recipe, summed up.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecipeGroup {
    pub recipe: String,
    pub machines: usize,
    /// PT: Item → produção por segundo a 100%. EN: Item → production per second at 100%.
    pub produces: BTreeMap<String, f64>,
    /// PT: Item → consumo por segundo a 100% (inclui combustível). EN: Item → consumption per second at 100% (includes fuel).
    pub consumes: BTreeMap<String, f64>,
    /// PT: Potência elétrica a 100%, sem dreno. EN: Electric power at 100%, without drain.
    pub power_w: f64,
    pub pollution_per_minute: f64,
}

/// PT: Agrupa as máquinas por receita e soma produção, consumo, potência e poluição.
/// EN: Groups the machines by recipe and sums production, consumption, power and pollution.
pub(crate) fn recipe_groups(
    data: &GameData,
    machines: &[Machine<'_>],
    fuel: &str,
) -> Vec<RecipeGroup> {
    let fuel_value = data
        .items
        .get(fuel)
        .and_then(|item| item.fuel.as_ref())
        .map_or(0.0, |f| f.value_j);
    let mut groups: BTreeMap<String, RecipeGroup> = BTreeMap::new();

    for machine in machines {
        let Some(recipe) = machine.recipe else {
            continue;
        };
        let report = &machine.report;
        let crafts = report.crafts_per_second;
        let group = groups
            .entry(recipe.name.clone())
            .or_insert_with(|| RecipeGroup {
                recipe: recipe.name.clone(),
                machines: 0,
                produces: BTreeMap::new(),
                consumes: BTreeMap::new(),
                power_w: 0.0,
                pollution_per_minute: 0.0,
            });
        group.machines += 1;
        for ingredient in &recipe.ingredients {
            *group.consumes.entry(ingredient.name.clone()).or_insert(0.0) +=
                ingredient.amount * crafts;
        }
        for product in &recipe.results {
            let expected = product.expected_amount();
            let productive = (expected - product.ignored_by_productivity).max(0.0);
            let amount = expected + productive * report.effect.productivity;
            *group.produces.entry(product.name.clone()).or_insert(0.0) += amount * crafts;
        }
        if report.electric {
            group.power_w += report.power_w;
        } else if is_burner(data, &report.name) && fuel_value > 0.0 {
            let effectivity = data
                .entities
                .get(&report.name)
                .and_then(|e| e.energy_source.as_ref())
                .map_or(1.0, |s| s.effectivity.max(1e-9));
            *group.consumes.entry(fuel.to_owned()).or_insert(0.0) +=
                report.power_w / effectivity / fuel_value;
        }
        group.pollution_per_minute += report.pollution_per_minute;
    }
    groups.into_values().collect()
}

fn is_burner(data: &GameData, name: &str) -> bool {
    data.entities
        .get(name)
        .and_then(|e| e.energy_source.as_ref())
        .is_some_and(|s| s.kind == EnergySourceKind::Burner)
}

/// PT: Balanço por item com cada grupo rodando na fração `utilization` (receita → 0..1).
///     Receita ausente no mapa roda a 100%.
/// EN: Per-item balance with each group running at fraction `utilization` (recipe → 0..1).
///     A recipe missing from the map runs at 100%.
pub fn balance(
    data: &GameData,
    groups: &[RecipeGroup],
    utilization: &BTreeMap<String, f64>,
) -> Vec<ItemFlow> {
    let mut flows: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for group in groups {
        let use_factor = utilization.get(&group.recipe).copied().unwrap_or(1.0);
        for (name, rate) in &group.produces {
            flows.entry(name.clone()).or_default().0 += rate * use_factor;
        }
        for (name, rate) in &group.consumes {
            flows.entry(name.clone()).or_default().1 += rate * use_factor;
        }
    }
    flows
        .into_iter()
        .map(|(name, (produced, consumed))| ItemFlow {
            kind: if data.is_fluid(&name) {
                ResourceKind::Fluid
            } else {
                ResourceKind::Item
            },
            name,
            produced_per_second: produced,
            consumed_per_second: consumed,
        })
        .collect()
}
