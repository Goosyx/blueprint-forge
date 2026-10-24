//! PT: Geração de energia: vapor (caldeira → motor, trocador de calor → turbina), solar,
//!     acumuladores e reatores. A produção elétrica do vapor é limitada pelo que vem antes na
//!     cadeia: água das bombas de água (quando houver), calor dos reatores e vapor por temperatura.
//! EN: Power generation: steam (boiler → engine, heat exchanger → turbine), solar, accumulators
//!     and reactors. Steam electric output is limited by what comes earlier in the chain: water
//!     from offshore pumps (when present), reactor heat and steam by temperature.

use std::collections::BTreeMap;

use bf_data::{EnergySourceKind, EntityKind, GameData};
use bf_world::World;
use serde::Serialize;

use crate::budget::{Budget, CalcError};

/// PT: Fator médio do painel solar em Nauvis (dia, noite e crepúsculo): 70% do pico.
/// EN: Average solar panel factor on Nauvis (day, night and dusk): 70% of peak.
pub const NAUVIS_SOLAR_AVERAGE: f64 = 0.7;
/// PT: Temperatura da água que entra na caldeira, em °C. EN: Temperature of water entering the boiler, in °C.
const WATER_TEMPERATURE: f64 = 15.0;

/// PT: Resumo da geração.
/// EN: Generation summary.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct GenerationReport {
    /// PT: Potência elétrica disponível agora (vapor limitado pela cadeia + solar no pico).
    /// EN: Electric power available now (chain-limited steam + solar at peak).
    pub electric_capacity_w: f64,
    pub steam_electric_w: f64,
    pub solar_peak_w: f64,
    pub solar_average_w: f64,
    pub accumulator_energy_j: f64,
    pub water_per_second: f64,
    /// PT: Temperatura do vapor → vapor por segundo que caldeiras e trocadores produzem.
    /// EN: Steam temperature → steam per second produced by boilers and exchangers.
    pub steam_per_second: BTreeMap<String, f64>,
    pub reactor_heat_w: f64,
    /// PT: Combustível das caldeiras por segundo (com o combustível escolhido).
    /// EN: Boiler fuel per second (with the chosen fuel).
    pub boiler_fuel_per_second: f64,
    /// PT: Poluição das caldeiras a queima, por minuto, com elas a 100%.
    /// EN: Burner boiler pollution, per minute, with them at 100%.
    pub pollution_per_minute: f64,
}

/// PT: Retângulo de tiles: x, y, largura, altura. EN: Tile rectangle: x, y, width, height.
type Rect = (i64, i64, u32, u32);

struct Boiler {
    temperature: f64,
    steam: f64,
    heat_powered: bool,
}

struct Generator {
    max_temperature: f64,
    steam_capacity: f64,
    effectivity: f64,
}

/// PT: Calcula a geração do mundo.
/// EN: Calculates the world's generation.
pub(crate) fn generation(
    data: &GameData,
    world: &World,
    fuel: &str,
    budget: &mut Budget,
) -> Result<GenerationReport, CalcError> {
    let mut report = GenerationReport::default();
    // PT: A caldeira aquece usando a capacidade térmica do fluido de saída (vapor, 0,2 kJ no 2.0),
    //     não a da água (2 kJ): 1,8 MW / (150 °C × 200 J) = 60 vapor/s, conferido no jogo.
    // EN: The boiler heats using the output fluid's heat capacity (steam, 0.2 kJ in 2.0), not the
    //     water's (2 kJ): 1.8 MW / (150 °C × 200 J) = 60 steam/s, checked in-game.
    let steam_heat = data
        .fluids
        .get("steam")
        .map_or(200.0, |f| f.heat_capacity_j);
    let fuel_value = data
        .items
        .get(fuel)
        .and_then(|item| item.fuel.as_ref())
        .map_or(0.0, |f| f.value_j);

    let mut boilers = Vec::new();
    let mut generators = Vec::new();
    let mut reactors: Vec<(Rect, f64, f64)> = Vec::new();
    let mut has_offshore = false;

    for placed in &world.entities {
        let Some(entity) = data.entities.get(&placed.name) else {
            continue;
        };
        match &entity.kind {
            EntityKind::OffshorePump { pumping_speed } => {
                has_offshore = true;
                report.water_per_second += pumping_speed * 60.0;
            }
            EntityKind::Boiler {
                target_temperature,
                energy_consumption_w,
            } => {
                let per_unit = (target_temperature - WATER_TEMPERATURE).max(1.0) * steam_heat;
                let source = entity.energy_source.as_ref();
                let heat_powered = source.is_some_and(|s| s.kind == EnergySourceKind::Heat);
                if let Some(source) = source.filter(|s| s.kind == EnergySourceKind::Burner) {
                    let effectivity = source.effectivity.max(1e-9);
                    if fuel_value > 0.0 {
                        report.boiler_fuel_per_second +=
                            energy_consumption_w / effectivity / fuel_value;
                    }
                    report.pollution_per_minute += source
                        .emissions_per_minute
                        .get("pollution")
                        .copied()
                        .unwrap_or(0.0);
                }
                boilers.push(Boiler {
                    temperature: *target_temperature,
                    steam: energy_consumption_w / per_unit,
                    heat_powered,
                });
            }
            EntityKind::Generator {
                fluid_usage_per_tick,
                maximum_temperature,
                effectivity,
            } => generators.push(Generator {
                max_temperature: *maximum_temperature,
                steam_capacity: fluid_usage_per_tick * 60.0,
                effectivity: *effectivity,
            }),
            EntityKind::SolarPanel { production_w } => report.solar_peak_w += production_w,
            EntityKind::Accumulator => {
                report.accumulator_energy_j += entity
                    .energy_source
                    .as_ref()
                    .and_then(|s| s.buffer_capacity_j)
                    .unwrap_or(0.0);
            }
            EntityKind::Reactor {
                consumption_w,
                neighbour_bonus,
            } => {
                let f = placed.footprint;
                reactors.push((
                    (f.x, f.y, f.width, f.height),
                    *consumption_w,
                    *neighbour_bonus,
                ));
            }
            _ => {}
        }
    }

    // PT: Bônus de vizinhança: cada reator encostado lado a lado em outro ganha `neighbour_bonus`.
    //     A comparação é entre pares, então gasta o orçamento ao quadrado.
    // EN: Neighbour bonus: each reactor touching another side by side gains `neighbour_bonus`.
    //     The comparison is pairwise, so it spends the budget squared.
    let count = u64::try_from(reactors.len()).unwrap_or(u64::MAX);
    budget.spend(count.saturating_mul(count))?;
    for (index, (rect, consumption, bonus)) in reactors.iter().enumerate() {
        let neighbours = reactors
            .iter()
            .enumerate()
            .filter(|(other, (other_rect, _, _))| {
                *other != index && touches_side(*rect, *other_rect)
            })
            .count();
        report.reactor_heat_w += consumption * (1.0 + bonus * neighbours as f64);
    }

    // PT: Limites da cadeia: o calor dos reatores limita os trocadores; a água das bombas (se a
    //     blueprint tiver bombas de água) limita todas as caldeiras, na proporção de 1 para 1.
    // EN: Chain limits: reactor heat limits the exchangers; offshore pump water (if the blueprint
    //     has offshore pumps) limits every boiler, at a 1 to 1 ratio.
    let heat_needed: f64 = boilers
        .iter()
        .filter(|b| b.heat_powered)
        .map(|b| b.steam * (b.temperature - WATER_TEMPERATURE).max(1.0) * steam_heat)
        .sum();
    let heat_factor = if heat_needed > 0.0 {
        (report.reactor_heat_w / heat_needed).min(1.0)
    } else {
        1.0
    };
    let mut supply: BTreeMap<i64, f64> = BTreeMap::new();
    for boiler in &boilers {
        let factor = if boiler.heat_powered {
            heat_factor
        } else {
            1.0
        };
        *supply
            .entry(boiler.temperature.round() as i64)
            .or_insert(0.0) += boiler.steam * factor;
    }
    let water_needed: f64 = supply.values().sum();
    if has_offshore && water_needed > report.water_per_second {
        let factor = report.water_per_second / water_needed.max(1e-9);
        for rate in supply.values_mut() {
            *rate *= factor;
        }
    }

    // PT: Geradores mais quentes primeiro pegam o vapor mais quente; a temperatura útil é a menor
    //     entre a do vapor e a máxima do gerador (vapor de 165 °C numa turbina rende menos).
    // EN: Hotter generators first take the hottest steam; the useful temperature is the lower of
    //     the steam's and the generator's maximum (165 °C steam in a turbine yields less).
    let mut remaining: Vec<(f64, f64)> =
        supply.iter().rev().map(|(t, r)| (*t as f64, *r)).collect();
    generators.sort_by(|a, b| b.max_temperature.total_cmp(&a.max_temperature));
    for generator in &generators {
        let mut capacity = generator.steam_capacity;
        for (temperature, rate) in &mut remaining {
            if capacity <= 0.0 {
                break;
            }
            let used = capacity.min(*rate);
            *rate -= used;
            capacity -= used;
            let useful = temperature.min(generator.max_temperature) - WATER_TEMPERATURE;
            report.steam_electric_w += used * useful.max(0.0) * steam_heat * generator.effectivity;
        }
    }

    report.steam_per_second = supply
        .into_iter()
        .map(|(temperature, rate)| (temperature.to_string(), rate))
        .collect();
    report.solar_average_w = report.solar_peak_w * NAUVIS_SOLAR_AVERAGE;
    report.electric_capacity_w = report.steam_electric_w + report.solar_peak_w;
    Ok(report)
}

fn touches_side(a: Rect, b: Rect) -> bool {
    let (ax, ay, aw, ah) = (a.0, a.1, i64::from(a.2), i64::from(a.3));
    let (bx, by, bw, bh) = (b.0, b.1, i64::from(b.2), i64::from(b.3));
    let horizontal = (ax + aw == bx || bx + bw == ax) && ay < by + bh && by < ay + ah;
    let vertical = (ay + ah == by || by + bh == ay) && ax < bx + bw && bx < ax + aw;
    horizontal || vertical
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_side_contact_only() {
        assert!(touches_side((0, 0, 5, 5), (5, 0, 5, 5)));
        assert!(touches_side((0, 0, 5, 5), (0, -5, 5, 5)));
        assert!(!touches_side((0, 0, 5, 5), (5, 5, 5, 5)));
        assert!(!touches_side((0, 0, 5, 5), (6, 0, 5, 5)));
    }
}
