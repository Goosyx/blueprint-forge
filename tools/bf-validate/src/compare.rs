//! PT: Compara o que o jogo mediu com o que o `bf-calc` calculou para a mesma blueprint: bônus de
//!     cada máquina, produção no regime estável, consumo elétrico por categoria e poluição.
//!     Tolerâncias do critério da Fase 2: bônus 1e-4, produção 1%, energia e poluição 2%.
//! EN: Compares what the game measured with what `bf-calc` calculated for the same blueprint:
//!     each machine's bonuses, steady-state production, electric consumption by category and
//!     pollution. Phase 2 criterion tolerances: bonuses 1e-4, production 1%, power and pollution 2%.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use bf_analysis::analyze;
use bf_blueprint::{BlueprintString, Limits, decode};
use bf_calc::AnalysisOptions;
use bf_data::GameData;
use serde_json::Value;

use crate::scenario::Prepared;

const BONUS_TOLERANCE: f64 = 1e-4;
const PRODUCTION_TOLERANCE: f64 = 0.01;
const POWER_TOLERANCE: f64 = 0.02;

fn verdict(ok: bool) -> &'static str {
    if ok { "OK   " } else { "FALHA" }
}

fn relative_error(measured: f64, predicted: f64) -> f64 {
    (measured - predicted).abs() / predicted.abs().max(1e-9)
}

/// PT: Roda o cálculo e imprime cada comparação; devolve `true` se tudo ficou na tolerância.
/// EN: Runs the calculation and prints each comparison; returns `true` if everything is within tolerance.
pub fn with_calculation(prepared: &Prepared, result: &Value, assets: &Path) -> Result<bool> {
    let text = fs::read_to_string(assets.join("gamedata.json")).with_context(|| {
        format!(
            "pacote do jogo ausente / game package missing: {}",
            assets.display()
        )
    })?;
    let data = GameData::from_json(&text)?;
    let limits = Limits::default();
    let options = AnalysisOptions {
        recipe_overrides: prepared.scenario.recipe_overrides.clone(),
        ..AnalysisOptions::default()
    };
    let analysis = analyze(&data, &prepared.blueprint, 0, &options, &limits)
        .map_err(|error| anyhow!("{error}"))?;
    let report = &analysis.report;
    let BlueprintString::Blueprint(blueprint) = decode(&prepared.blueprint, &limits)
        .map_err(|error| anyhow!("{error}"))?
        .content
    else {
        bail!("cenário sem blueprint / scenario without a blueprint");
    };
    let positions: HashMap<u64, (f64, f64)> = blueprint
        .entities
        .iter()
        .map(|entity| (entity.entity_number, (entity.position.x, entity.position.y)))
        .collect();

    let mut passed = true;
    println!("\n== jogo × cálculo / game × calculation ==");

    // PT: Bônus de cada máquina, achada pelo nome e pela posição.
    // EN: Bonuses of each machine, matched by name and position.
    let measured_machines = result
        .get("machines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // PT: As duas listas precisam ter o mesmo tamanho: uma lista vazia ou incompleta do cálculo
    //     (protótipo que deixou de ser reconhecido) faria a comparação passar sem testar nada.
    // EN: Both lists must have the same size: an empty or incomplete calculation list (a prototype
    //     no longer recognized) would make the comparison pass without testing anything.
    if report.machines.len() != measured_machines.len() {
        println!(
            "FALHA máquinas: jogo/game {}, cálculo/calc {} / machine count differs",
            measured_machines.len(),
            report.machines.len()
        );
        passed = false;
    }
    for machine in &report.machines {
        let Some((x, y)) = positions.get(&machine.entity_number) else {
            continue;
        };
        let found = measured_machines.iter().find(|m| {
            m.get("name").and_then(Value::as_str) == Some(machine.name.as_str())
                && (m.get("x").and_then(Value::as_f64).unwrap_or(f64::NAN) - x).abs() < 0.01
                && (m.get("y").and_then(Value::as_f64).unwrap_or(f64::NAN) - y).abs() < 0.01
        });
        let Some(found) = found else {
            println!(
                "FALHA máquina {} não encontrada no jogo / machine not found in game",
                machine.entity_number
            );
            passed = false;
            continue;
        };
        for (field, predicted) in [
            ("speed_bonus", machine.effect.speed),
            ("productivity_bonus", machine.effect.productivity),
            ("consumption_bonus", machine.effect.consumption),
            ("pollution_bonus", machine.effect.pollution),
        ] {
            let measured = found.get(field).and_then(Value::as_f64).unwrap_or(f64::NAN);
            let ok = (measured - predicted).abs() <= BONUS_TOLERANCE;
            passed &= ok;
            println!(
                "{} máquina/machine {} {field}: jogo/game {measured:.4}, cálculo/calc {predicted:.4}",
                verdict(ok),
                machine.entity_number
            );
        }
    }

    if prepared.scenario.compare_bonuses_only {
        return Ok(passed);
    }

    // PT: Produção no regime estável. EN: Steady-state production.
    for item in &prepared.scenario.measure {
        // PT: Vapor vem das caldeiras (geração), não de receitas. EN: Steam comes from boilers (generation), not recipes.
        let predicted = if item == "steam" {
            report.generation.steam_per_second.values().sum()
        } else {
            report
                .steady
                .iter()
                .find(|flow| &flow.name == item)
                .map_or(0.0, |flow| flow.produced_per_second)
        };
        let measured = result
            .pointer(&format!("/rates/{item}/produced_per_second"))
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN);
        // PT: Só pula quando os dois são zero; previsão 0 com produção real é falha.
        // EN: Only skips when both are zero; a prediction of 0 with real production is a failure.
        if predicted <= 0.0 && measured <= 0.0 {
            continue;
        }
        let error = relative_error(measured, predicted);
        let ok = error <= PRODUCTION_TOLERANCE;
        passed &= ok;
        println!(
            "{} produção/production {item}: jogo/game {measured:.4}/s, cálculo/calc {predicted:.4}/s ({:.2}%)",
            verdict(ok),
            error * 100.0
        );
    }

    // PT: Consumo elétrico: o dicionário de consumo é o que não contém a interface de energia.
    // EN: Electric consumption: the consumption dictionary is the one without the energy interface.
    let input = result.get("electric_input_w").and_then(Value::as_object);
    let output = result.get("electric_output_w").and_then(Value::as_object);
    let consumption = match (input, output) {
        (Some(i), _) if !i.contains_key("electric-energy-interface") => Some(i),
        (_, Some(o)) if !o.contains_key("electric-energy-interface") => Some(o),
        _ => None,
    };
    let names: std::collections::BTreeSet<&str> =
        report.machines.iter().map(|m| m.name.as_str()).collect();
    if let Some(consumption) = consumption {
        let sum = |filter: &dyn Fn(&str) -> bool| -> f64 {
            consumption
                .iter()
                .filter(|(name, _)| filter(name))
                .filter_map(|(_, value)| value.as_f64())
                .sum()
        };
        for (category, measured) in [
            ("crafting", sum(&|name| names.contains(name))),
            ("beacon", sum(&|name| name == "beacon")),
        ] {
            let predicted = report
                .power
                .by_category
                .get(category)
                .copied()
                .unwrap_or(0.0);
            if predicted <= 0.0 && measured <= 0.0 {
                continue;
            }
            let error = relative_error(measured, predicted);
            let ok = error <= POWER_TOLERANCE;
            passed &= ok;
            println!(
                "{} energia/power {category}: jogo/game {:.1} kW, cálculo/calc {:.1} kW ({:.2}%)",
                verdict(ok),
                measured / 1000.0,
                predicted / 1000.0,
                error * 100.0
            );
        }
    } else {
        println!("FALHA estatística elétrica ausente / electric statistics missing");
        passed = false;
    }

    // PT: Geração a vapor: no modo `load`, a produção elétrica dos geradores contra o cálculo.
    // EN: Steam generation: in `load` mode, the generators' electric output against the calculation.
    if prepared.scenario.power == crate::scenario::PowerMode::Load {
        let production = match (input, output) {
            (Some(i), _) if i.contains_key("steam-engine") || i.contains_key("steam-turbine") => {
                Some(i)
            }
            (_, Some(o)) if o.contains_key("steam-engine") || o.contains_key("steam-turbine") => {
                Some(o)
            }
            _ => None,
        };
        let production = production.or(match (input, output) {
            (Some(i), _) if i.contains_key("solar-panel") => Some(i),
            (_, Some(o)) if o.contains_key("solar-panel") => Some(o),
            _ => None,
        });
        let sum = |names: &[&str]| -> f64 {
            production
                .map(|p| {
                    p.iter()
                        .filter(|(name, _)| names.contains(&name.as_str()))
                        .filter_map(|(_, value)| value.as_f64())
                        .sum()
                })
                .unwrap_or(0.0)
        };
        // PT: A superfície de teste é sempre de dia, então o solar entrega o pico.
        // EN: The test surface is always day, so solar delivers its peak.
        for (label, measured, predicted) in [
            (
                "vapor/steam",
                sum(&["steam-engine", "steam-turbine"]),
                report.generation.steam_electric_w,
            ),
            (
                "solar",
                sum(&["solar-panel"]),
                report.generation.solar_peak_w,
            ),
        ] {
            if predicted <= 0.0 && measured <= 0.0 {
                continue;
            }
            let error = relative_error(measured, predicted);
            let ok = error <= POWER_TOLERANCE;
            passed &= ok;
            println!(
                "{} geração/generation {label}: jogo/game {:.1} kW, cálculo/calc {:.1} kW ({:.2}%)",
                verdict(ok),
                measured / 1000.0,
                predicted / 1000.0,
                error * 100.0
            );
        }
    }

    // PT: Poluição por minuto. EN: Pollution per minute.
    let pollution = result
        .get("pollution_per_second")
        .and_then(Value::as_object);
    if let Some(pollution) = pollution {
        // PT: Poluição total de todas as entidades (máquinas, caldeiras, mineradoras).
        // EN: Total pollution of every entity (machines, boilers, mining drills).
        let measured: f64 = pollution.values().filter_map(Value::as_f64).sum::<f64>() * 60.0;
        let predicted = report.pollution_per_minute;
        if predicted > 0.0 || measured > 0.0 {
            let error = relative_error(measured, predicted);
            let ok = error <= POWER_TOLERANCE;
            passed &= ok;
            println!(
                "{} poluição/pollution: jogo/game {measured:.3}/min, cálculo/calc {predicted:.3}/min ({:.2}%)",
                verdict(ok),
                error * 100.0
            );
        }
    }
    Ok(passed)
}
