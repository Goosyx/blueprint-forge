//! PT: Cálculo por máquina de produção: receita usada, módulos, beacons que a alcançam, efeito
//!     final, ciclos por segundo, potência, dreno e poluição. Tudo a 100% de uso; o uso real sai
//!     da programação linear (`lp.rs`).
//! EN: Per crafting machine calculation: recipe used, modules, beacons reaching it, final effect,
//!     cycles per second, power, drain and pollution. Everything at 100% use; the real use comes
//!     from linear programming (`lp.rs`).

use std::collections::{BTreeMap, HashMap, HashSet};

use bf_blueprint::Blueprint;
use bf_data::{Effect, EnergySourceKind, EntityKind, GameData, Recipe};
use bf_world::power::PowerCoverage;
use bf_world::{Footprint, World};
use serde::Serialize;

use crate::CalcWarning;
use crate::budget::{Budget, CalcError};
use crate::effects::{add_scaled, clamp, filter_allowed, modules_effect, profile_factor};

/// PT: Lado do bloco do índice espacial, em tiles.
/// EN: Side of the spatial index block, in tiles.
const CHUNK: i64 = 8;

/// PT: Origem da receita de uma máquina.
/// EN: Origin of a machine's recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSource {
    Blueprint,
    Override,
    Fixed,
    /// PT: Deduzida pelos itens que chegam (fornalhas). EN: Inferred from the arriving items (furnaces).
    Inferred,
}

/// PT: Resultado de uma máquina.
/// EN: Result for one machine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MachineReport {
    pub entity_number: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe_source: Option<RecipeSource>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, u32>,
    pub beacons: usize,
    pub effect: Effect,
    /// PT: Ciclos da receita por segundo a 100%. EN: Recipe cycles per second at 100%.
    pub crafts_per_second: f64,
    /// PT: Potência em funcionamento, sem o dreno. EN: Power while working, without the drain.
    pub power_w: f64,
    pub drain_w: f64,
    pub electric: bool,
    /// PT: Falso quando é elétrica e nenhum poste a alcança (então não produz nem consome).
    /// EN: False when it is electric and no pole reaches it (then it neither produces nor consumes).
    pub powered: bool,
    pub pollution_per_minute: f64,
}

/// PT: Uma máquina já calculada, com a receita resolvida (uso interno do crate).
/// EN: An already calculated machine, with its recipe resolved (crate-internal use).
#[derive(Debug, Clone)]
pub(crate) struct Machine<'a> {
    pub report: MachineReport,
    pub recipe: Option<&'a Recipe>,
}

/// PT: Módulos de uma entidade da blueprint (só itens que são módulos), limitados aos `slots` da
///     entidade: uma blueprint hostil não consegue pôr 1000 módulos numa máquina de 4 vagas.
/// EN: Modules of a blueprint entity (only items that are modules), limited to the entity's
///     `slots`: a hostile blueprint cannot put 1000 modules into a 4-slot machine.
pub(crate) fn entity_modules(
    data: &GameData,
    blueprint: &Blueprint,
    index: usize,
    slots: u32,
) -> BTreeMap<String, u32> {
    let mut left = slots;
    let mut modules = BTreeMap::new();
    let counts = blueprint
        .entities
        .get(index)
        .and_then(|entity| entity.items.as_ref())
        .map(|items| items.counts())
        .unwrap_or_default();
    for (name, count) in counts {
        if left == 0 {
            break;
        }
        if data
            .items
            .get(&name)
            .is_some_and(|item| item.module.is_some())
        {
            let taken = count.min(left);
            left -= taken;
            modules.insert(name, taken);
        }
    }
    modules
}

/// PT: Calcula todas as máquinas de produção do mundo.
/// EN: Calculates every crafting machine in the world.
pub(crate) fn machines<'a>(
    data: &'a GameData,
    blueprint: &Blueprint,
    world: &World,
    poles: &PowerCoverage,
    choices: &BTreeMap<u64, (String, RecipeSource)>,
    warnings: &mut Vec<CalcWarning>,
    budget: &mut Budget,
) -> Result<Vec<Machine<'a>>, CalcError> {
    let beacons = beacon_coverage(data, blueprint, world, poles, budget)?;
    let mut result = Vec::new();

    for (slot, placed) in world.entities.iter().enumerate() {
        let Some(entity) = data.entities.get(&placed.name) else {
            continue;
        };
        let EntityKind::CraftingMachine {
            crafting_speed,
            crafting_categories,
            allowed_effects,
            uses_module_effects,
            fixed_recipe,
            module_slots,
            ..
        } = &entity.kind
        else {
            continue;
        };
        let modules = entity_modules(data, blueprint, placed.index, *module_slots);

        // PT: Receita: a escolhida pelo usuário, a da blueprint ou a fixa da máquina.
        // EN: Recipe: the one chosen by the user, the blueprint's or the machine's fixed one.
        let choice = choices.get(&placed.entity_number);
        let chosen = if let Some((name, RecipeSource::Override)) = choice {
            Some((name.clone(), RecipeSource::Override))
        } else if let Some(name) = &placed.recipe {
            Some((name.clone(), RecipeSource::Blueprint))
        } else if let Some(name) = fixed_recipe {
            Some((name.clone(), RecipeSource::Fixed))
        } else {
            choice.cloned()
        };

        let mut recipe_source = None;
        let recipe = match chosen {
            None => {
                warnings.push(CalcWarning::NoRecipe {
                    entity_number: placed.entity_number,
                });
                None
            }
            Some((name, source)) => match data.recipes.get(&name) {
                Some(recipe) if fits(recipe, crafting_categories) => {
                    recipe_source = Some(source);
                    Some(recipe)
                }
                Some(_) => {
                    warnings.push(CalcWarning::RecipeNotAllowed {
                        entity_number: placed.entity_number,
                        recipe: name,
                    });
                    None
                }
                None => {
                    warnings.push(CalcWarning::UnknownRecipe {
                        entity_number: placed.entity_number,
                        recipe: name,
                    });
                    None
                }
            },
        };

        let mut effect = if *uses_module_effects {
            modules_effect(data, &modules, allowed_effects)
        } else {
            Effect::default()
        };
        let coverage = beacons.get(&slot);
        if let Some(coverage) = coverage {
            add_scaled(
                &mut effect,
                &filter_allowed(coverage.effect, allowed_effects),
                1.0,
            );
        }
        let effect = match recipe {
            Some(recipe) => clamp(
                effect,
                recipe.allow_productivity,
                recipe.maximum_productivity,
            ),
            // PT: Sem receita o jogo ainda mostra a produtividade dos módulos (teto padrão de
            //     300%, conferido no jogo); a máquina parada não produz nada de qualquer jeito.
            // EN: Without a recipe the game still shows the modules' productivity (default 300%
            //     cap, checked in-game); the idle machine produces nothing either way.
            None => clamp(effect, true, bf_data::DEFAULT_MAXIMUM_PRODUCTIVITY),
        };

        let powered = poles.powered(slot);
        if !powered {
            warnings.push(CalcWarning::Unpowered {
                entity_number: placed.entity_number,
            });
        }
        let crafts_per_second = match recipe {
            Some(recipe) if powered && recipe.energy_required > 0.0 => {
                crafting_speed * (1.0 + effect.speed) / recipe.energy_required
            }
            _ => 0.0,
        };

        let usage = entity.energy_usage_w.unwrap_or(0.0);
        let energy = entity.energy_source.as_ref();
        let electric = energy.is_some_and(|s| s.kind == EnergySourceKind::Electric);
        // PT: Máquinas elétricas sem dreno declarado drenam 1/30 do consumo.
        // EN: Electric machines without a declared drain drain 1/30 of their usage.
        let drain_w = match energy {
            Some(s) if electric => s.drain_w.unwrap_or(usage / 30.0),
            _ => 0.0,
        };
        let base_pollution = energy
            .and_then(|s| s.emissions_per_minute.get("pollution"))
            .copied()
            .unwrap_or(0.0);
        let multiplier = recipe.map_or(1.0, |r| r.emissions_multiplier);

        result.push(Machine {
            report: MachineReport {
                entity_number: placed.entity_number,
                name: placed.name.clone(),
                recipe: recipe.map(|r| r.name.clone()),
                recipe_source,
                modules,
                beacons: coverage.map_or(0, |c| c.count),
                effect,
                crafts_per_second,
                power_w: if powered {
                    usage * (1.0 + effect.consumption)
                } else {
                    0.0
                },
                drain_w: if powered { drain_w } else { 0.0 },
                electric,
                powered,
                pollution_per_minute: base_pollution
                    * (1.0 + effect.consumption)
                    * (1.0 + effect.pollution)
                    * multiplier,
            },
            recipe,
        });
    }
    Ok(result)
}

/// PT: Beacons que alcançam uma máquina e o efeito somado que eles transmitem.
/// EN: Beacons reaching a machine and the combined effect they transmit.
#[derive(Debug, Clone, Copy, Default)]
struct Coverage {
    count: usize,
    effect: Effect,
}

// PT: Para cada máquina que aceita efeitos de beacon, conta os beacons cuja área de alcance
//     (área do beacon aumentada por `supply_area_distance`) toca a máquina, e soma o efeito
//     transmitido com o fator do perfil. Usa um índice espacial por blocos para não comparar
//     todos com todos.
// EN: For each machine that accepts beacon effects, counts the beacons whose reach area (the
//     beacon's area grown by `supply_area_distance`) touches the machine, and adds the transmitted
//     effect with the profile factor. Uses a block spatial index to avoid comparing all pairs.
fn beacon_coverage(
    data: &GameData,
    blueprint: &Blueprint,
    world: &World,
    poles: &PowerCoverage,
    budget: &mut Budget,
) -> Result<HashMap<usize, Coverage>, CalcError> {
    let mut index: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (slot, placed) in world.entities.iter().enumerate() {
        let accepts = data.entities.get(&placed.name).is_some_and(|entity| {
            matches!(
                entity.kind,
                EntityKind::CraftingMachine {
                    uses_beacon_effects: true,
                    ..
                }
            )
        });
        if accepts {
            for block in blocks(&placed.footprint, 0.0) {
                budget.spend(1)?;
                index.entry(block).or_default().push(slot);
            }
        }
    }

    // PT: Por máquina: quantos beacons a alcançam, a soma do que transmitem (já com
    //     `distribution_effectivity`) e o perfil. Memória proporcional às máquinas, não aos pares.
    // EN: Per machine: how many beacons reach it, the sum of what they transmit (already with
    //     `distribution_effectivity`) and the profile. Memory proportional to machines, not pairs.
    let mut reached: HashMap<usize, (usize, Effect, &[f64])> = HashMap::new();
    for (beacon_slot, placed) in world.entities.iter().enumerate() {
        // PT: Beacon sem energia não transmite nada. EN: An unpowered beacon transmits nothing.
        if !poles.powered(beacon_slot) {
            continue;
        }
        let Some(entity) = data.entities.get(&placed.name) else {
            continue;
        };
        let EntityKind::Beacon {
            supply_area_distance,
            distribution_effectivity,
            profile,
            allowed_effects,
            module_slots,
        } = &entity.kind
        else {
            continue;
        };
        let modules = entity_modules(data, blueprint, placed.index, *module_slots);
        let mut transmitted = Effect::default();
        add_scaled(
            &mut transmitted,
            &modules_effect(data, &modules, allowed_effects),
            *distribution_effectivity,
        );

        let mut seen = HashSet::new();
        for block in blocks(&placed.footprint, *supply_area_distance) {
            for &slot in index.get(&block).map_or(&[][..], Vec::as_slice) {
                // PT: Cada par beacon × máquina examinado gasta um passo do orçamento.
                // EN: Each examined beacon × machine pair spends one budget step.
                budget.spend(1)?;
                if !seen.insert(slot) {
                    continue;
                }
                let Some(machine) = world.entities.get(slot) else {
                    continue;
                };
                if overlaps(&placed.footprint, *supply_area_distance, &machine.footprint) {
                    let entry =
                        reached
                            .entry(slot)
                            .or_insert((0, Effect::default(), profile.as_slice()));
                    entry.0 += 1;
                    add_scaled(&mut entry.1, &transmitted, 1.0);
                }
            }
        }
    }

    Ok(reached
        .into_iter()
        .map(|(slot, (count, sum, profile))| {
            let mut total = Effect::default();
            add_scaled(&mut total, &sum, profile_factor(profile, count));
            (
                slot,
                Coverage {
                    count,
                    effect: total,
                },
            )
        })
        .collect())
}

// PT: Blocos do índice cobertos pela área aumentada em `grow` tiles.
// EN: Index blocks covered by the area grown by `grow` tiles.
fn blocks(footprint: &Footprint, grow: f64) -> Vec<(i64, i64)> {
    let grow = grow.ceil() as i64;
    let x1 = (footprint.x - grow).div_euclid(CHUNK);
    let y1 = (footprint.y - grow).div_euclid(CHUNK);
    let x2 = (footprint.x + i64::from(footprint.width) + grow).div_euclid(CHUNK);
    let y2 = (footprint.y + i64::from(footprint.height) + grow).div_euclid(CHUNK);
    let mut result = Vec::new();
    for bx in x1..=x2 {
        for by in y1..=y2 {
            result.push((bx, by));
        }
    }
    result
}

// PT: A área do beacon aumentada em `grow` tem interseção (com área positiva) com a máquina?
// EN: Does the beacon area grown by `grow` intersect (with positive area) the machine?
fn overlaps(beacon: &Footprint, grow: f64, machine: &Footprint) -> bool {
    let bx1 = beacon.x as f64 - grow;
    let by1 = beacon.y as f64 - grow;
    let bx2 = (beacon.x + i64::from(beacon.width)) as f64 + grow;
    let by2 = (beacon.y + i64::from(beacon.height)) as f64 + grow;
    let mx1 = machine.x as f64;
    let my1 = machine.y as f64;
    let mx2 = (machine.x + i64::from(machine.width)) as f64;
    let my2 = (machine.y + i64::from(machine.height)) as f64;
    bx1 < mx2 && mx1 < bx2 && by1 < my2 && my1 < by2
}

// PT: A máquina faz a receita se tiver qualquer uma das categorias dela (2.1+).
// EN: The machine crafts the recipe if it has any of its categories (2.1+).
fn fits(recipe: &Recipe, crafting_categories: &[String]) -> bool {
    recipe
        .categories
        .iter()
        .any(|category| crafting_categories.contains(category))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_fits_any_of_its_categories() {
        let recipe: Recipe = serde_json::from_value(serde_json::json!({
            "name": "r", "categories": ["a", "b"], "energy_required": 1.0,
            "ingredients": [], "results": [], "allow_productivity": false,
            "maximum_productivity": 3.0, "emissions_multiplier": 1.0,
            "enabled": true, "hidden": false
        }))
        .unwrap();
        assert!(fits(&recipe, &["b".to_owned()]));
        assert!(fits(&recipe, &["x".to_owned(), "a".to_owned()]));
        assert!(!fits(&recipe, &["c".to_owned()]));
        assert!(!fits(&recipe, &[]));
    }

    const fn rect(x: i64, y: i64, width: u32, height: u32) -> Footprint {
        Footprint {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn beacon_reach_touches_neighbors_only() {
        let beacon = rect(0, 0, 3, 3);
        // PT: Alcance 3: de -3 até 6. EN: Reach 3: from -3 to 6.
        assert!(overlaps(&beacon, 3.0, &rect(3, 0, 3, 3)));
        assert!(overlaps(&beacon, 3.0, &rect(5, 5, 3, 3)));
        assert!(!overlaps(&beacon, 3.0, &rect(6, 0, 3, 3)));
        assert!(!overlaps(&beacon, 3.0, &rect(-6, -6, 3, 3)));
    }

    #[test]
    fn blocks_cover_negative_coordinates() {
        let found = blocks(&rect(-1, -1, 2, 2), 0.0);
        assert!(found.contains(&(-1, -1)));
        assert!(found.contains(&(0, 0)));
    }
}
