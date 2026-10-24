//! PT: Compara as simulações de fluxo com o jogo: roda cada cenário marcado com `sim` no headless,
//!     simula a mesma blueprint com cada versão e grava uma tabela com o valor do jogo, o de cada
//!     simulação, o erro e o tempo de cada uma em `validation/measurements/sim-comparison.md`.
//! EN: Compares the flow simulations with the game: runs each scenario marked with `sim` on the
//!     headless server, simulates the same blueprint with each version and writes a table with
//!     the game value, each simulation's value, the error and each one's time into
//!     `validation/measurements/sim-comparison.md`.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use bf_blueprint::{BlueprintString, Limits, decode};
use bf_calc::{AnalysisOptions, calculate};
use bf_data::GameData;
use bf_world::{Topology, World};
use serde_json::Value;

use crate::execute;
use crate::scenario::{Prepared, prepare, resolve_file};

/// PT: Uma versão de simulação: nome e função. EN: One simulation version: name and function.
pub type Simulator = (
    &'static str,
    fn(&bf_sim::SimInput<'_>, u32, u32) -> Result<bf_sim::FlowReport, bf_sim::SimError>,
);

/// PT: Previsão de uma versão: nome, item → itens por segundo, tempo em ms.
/// EN: One version's prediction: name, item → items per second, time in ms.
pub type Prediction = (&'static str, std::collections::BTreeMap<String, f64>, f64);

/// PT: Uma linha da tabela. EN: One table row.
pub struct Row {
    pub scenario: String,
    pub item: String,
    pub game: f64,
    /// PT: Cenário de travamento (fora da conta do regime estável). EN: Deadlock scenario (left out of the steady-state check).
    pub deadlock: bool,
    /// PT: (versão, previsão, tempo em ms). EN: (version, prediction, time in ms).
    pub predictions: Vec<(&'static str, f64, f64)>,
}

/// PT: Tolerâncias do critério: erro por linha e erro médio, por versão.
/// EN: Criterion tolerances: per-row error and mean error, per version.
const STEADY_ROW: f64 = 0.02;
const TICK_ROW: f64 = 0.15;
const TICK_MEAN: f64 = 0.05;

/// PT: Previsões das simulações para os itens medidos de um cenário.
/// EN: The simulations' predictions for a scenario's measured items.
pub fn predict(
    prepared: &Prepared,
    data: &GameData,
    simulators: &[Simulator],
) -> Result<Vec<Prediction>> {
    let limits = Limits::default();
    let BlueprintString::Blueprint(blueprint) = decode(&prepared.blueprint, &limits)
        .map_err(|e| anyhow!("{e}"))?
        .content
    else {
        bail!("cenário sem blueprint / scenario without a blueprint");
    };
    let world = World::build(&blueprint, data).map_err(|e| anyhow!("{e}"))?;
    let topology = Topology::build(&world, &blueprint, data);
    let options = AnalysisOptions {
        recipe_overrides: prepared.scenario.recipe_overrides.clone(),
        inserter_research: match prepared.scenario.research {
            crate::scenario::Research::All => bf_calc::InserterResearch::All,
            crate::scenario::Research::None => bf_calc::InserterResearch::None,
        },
        ..AnalysisOptions::default()
    };
    let report = calculate(data, &blueprint, &world, &options).map_err(|e| anyhow!("{e}"))?;
    let fuel = prepared.scenario.prefuel.as_deref().unwrap_or("coal");
    let input = bf_sim::build(
        data,
        &world,
        &topology,
        &report,
        options.inserter_research,
        fuel,
    );
    let mut out = Vec::new();
    for (name, simulate) in simulators {
        let started = Instant::now();
        let flow = simulate(
            &input,
            prepared.scenario.warmup_ticks,
            prepared.scenario.measure_ticks,
        )
        .map_err(|e| anyhow!("{e}"))?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        out.push((*name, flow.produced, ms));
    }
    Ok(out)
}

/// PT: Roda a comparação para todos os cenários `sim` das pastas. EN: Runs the comparison for every `sim` scenario in the folders.
pub fn run(
    dirs: &[PathBuf],
    headless: &Path,
    work: &Path,
    mod_dir: &Path,
    assets: &Path,
    out: &Path,
    simulators: &[Simulator],
) -> Result<Vec<Row>> {
    let data = GameData::from_json(&fs::read_to_string(assets.join("gamedata.json"))?)?;
    let mut files = Vec::new();
    for dir in dirs {
        for entry in fs::read_dir(dir).with_context(|| dir.display().to_string())? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut rows = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path)?;
        let validation_dir = path
            .parent()
            .and_then(|p| {
                if p.ends_with("scenarios") {
                    p.parent()
                } else {
                    p.parent().and_then(Path::parent)
                }
            })
            .unwrap_or_else(|| Path::new("."));
        let scenario = resolve_file(serde_json::from_str(&text)?, validation_dir)?;
        if !scenario.sim {
            continue;
        }
        let prepared = prepare(scenario)?;
        println!("== {}", prepared.scenario.id);
        let result = execute(&prepared, headless, work, mod_dir)?;
        let predictions = predict(&prepared, &data, simulators)?;
        for item in &prepared.scenario.measure {
            let game = result
                .pointer(&format!("/rates/{item}/produced_per_second"))
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            let row = Row {
                scenario: prepared.scenario.id.clone(),
                item: item.clone(),
                game,
                deadlock: prepared.scenario.deadlock,
                predictions: predictions
                    .iter()
                    .map(|(name, produced, ms)| {
                        (*name, produced.get(item).copied().unwrap_or(0.0), *ms)
                    })
                    .collect(),
            };
            let mut line = format!("   {:<24} jogo/game {:>8.4}", row.item, row.game);
            for (name, value, ms) in &row.predictions {
                let _ = write!(
                    line,
                    " | {name} {value:>8.4} ({:>6.2}%, {ms:.2} ms)",
                    error(*value, game) * 100.0
                );
            }
            println!("{line}");
            rows.push(row);
        }
    }
    fs::write(out, markdown(&rows, simulators))?;
    Ok(rows)
}

/// PT: Confere as tolerâncias: o regime estável em cada linha que não é travamento, e a
///     simulação por tick em cada linha e na média. Imprime o que falhou.
/// EN: Checks the tolerances: the steady state on each non-deadlock row, and the tick simulation
///     on each row and on average. Prints what failed.
pub fn passes(rows: &[Row]) -> bool {
    let mut ok = true;
    let mut tick_sum = 0.0;
    let mut tick_count = 0usize;
    for row in rows {
        // PT: Sem medida do jogo não há comparação: é falha, não aprovação.
        // EN: Without a game measurement there is no comparison: it is a failure, not a pass.
        if !row.game.is_finite() {
            println!(
                "FALHA {} {}: sem medida do jogo / no game measurement",
                row.scenario, row.item
            );
            ok = false;
            continue;
        }
        for (name, value, _) in &row.predictions {
            let e = error(*value, row.game);
            let limit = match *name {
                "steady" if row.deadlock => continue,
                "steady" => STEADY_ROW,
                _ => {
                    tick_sum += e;
                    tick_count += 1;
                    TICK_ROW
                }
            };
            if e > limit {
                println!(
                    "FALHA {name} {} {}: {:.2}% > {:.0}%",
                    row.scenario,
                    row.item,
                    e * 100.0,
                    limit * 100.0
                );
                ok = false;
            }
        }
    }
    let mean = if tick_count > 0 {
        tick_sum / tick_count as f64
    } else {
        0.0
    };
    if mean > TICK_MEAN {
        println!(
            "FALHA tick: erro médio / mean error {:.2}% > {:.0}%",
            mean * 100.0,
            TICK_MEAN * 100.0
        );
        ok = false;
    }
    ok
}

fn error(predicted: f64, game: f64) -> f64 {
    if game.abs() < 1e-9 {
        if predicted.abs() < 1e-9 { 0.0 } else { 1.0 }
    } else {
        (predicted - game).abs() / game.abs()
    }
}

fn markdown(rows: &[Row], simulators: &[Simulator]) -> String {
    let mut text = String::from(
        "# Simulações de fluxo × jogo / Flow simulations × game\n\n\
         Gerado por / Generated by `bf-validate compare-sim`.\n\n",
    );
    let _ = write!(text, "| Cenário / Scenario | Item | Jogo / Game |");
    for (name, _) in simulators {
        let _ = write!(text, " {name} | erro / error | ms |");
    }
    text.push('\n');
    text.push_str("|---|---|---:|");
    for _ in simulators {
        text.push_str("---:|---:|---:|");
    }
    text.push('\n');
    let mut totals = vec![(0.0, 0usize); simulators.len()];
    let mut without_deadlock = vec![(0.0, 0usize); simulators.len()];
    for row in rows {
        let _ = write!(
            text,
            "| {} | {} | {:.4} |",
            row.scenario, row.item, row.game
        );
        for (index, (_, value, ms)) in row.predictions.iter().enumerate() {
            let e = error(*value, row.game);
            if let Some(total) = totals.get_mut(index) {
                total.0 += e;
                total.1 += 1;
            }
            if !row.deadlock
                && let Some(total) = without_deadlock.get_mut(index)
            {
                total.0 += e;
                total.1 += 1;
            }
            let _ = write!(text, " {value:.4} | {:.2}% | {ms:.2} |", e * 100.0);
        }
        text.push('\n');
    }
    text.push('\n');
    let mean = |(sum, count): (f64, usize)| {
        if count > 0 {
            sum / count as f64 * 100.0
        } else {
            0.0
        }
    };
    for (((name, _), all), clean) in simulators.iter().zip(totals).zip(without_deadlock) {
        let _ = writeln!(
            text,
            "- **{name}**: erro médio / mean error {:.2}% (sem os cenários de travamento / \
             without the deadlock scenarios: {:.2}%)",
            mean(all),
            mean(clean)
        );
    }

    text
}
