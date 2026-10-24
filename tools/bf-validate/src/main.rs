//! PT: `bf-validate` roda um cenário dentro do Factorio headless oficial: prepara uma instância
//!     isolada com o mod `bf-validator`, cria um mapa, roda N ticks em modo benchmark e compara a
//!     produção medida com a esperada.
//! EN: `bf-validate` runs a scenario inside the official Factorio headless server: it prepares an
//!     isolated instance with the `bf-validator` mod, creates a map, runs N ticks in benchmark mode
//!     and compares the measured production with the expected one.

mod bench;
mod calibrate;
mod compare;
mod scenario;
mod simcompare;
mod throughput;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser)]
#[command(version, about = "Validação dentro do jogo / In-game validation")]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand)]
enum CommandKind {
    /// PT: Roda um cenário. EN: Runs one scenario.
    Run {
        /// PT: Arquivo JSON do cenário. EN: Scenario JSON file.
        scenario: PathBuf,
        /// PT: Pasta do headless. EN: Headless folder.
        #[arg(long)]
        headless: Option<PathBuf>,
        /// PT: Pasta de trabalho. EN: Work folder.
        #[arg(long, default_value = "target/bf-validate")]
        work: PathBuf,
        /// PT: Pasta do mod. EN: Mod folder.
        #[arg(long, default_value = "mod/bf-validator")]
        mod_dir: PathBuf,
        /// PT: Pacote do jogo para comparar com o cálculo (padrão: `game-assets/<versão>`).
        /// EN: Game package to compare with the calculation (default: `game-assets/<version>`).
        #[arg(long)]
        assets: Option<PathBuf>,
    },
    /// PT: Compara as simulações de fluxo com o jogo. EN: Compares the flow simulations with the game.
    CompareSim {
        /// PT: Pastas com cenários. EN: Folders with scenarios.
        #[arg(default_values = ["validation/scenarios", "validation/scenarios/flow"])]
        dirs: Vec<PathBuf>,
        #[arg(long)]
        headless: Option<PathBuf>,
        #[arg(long, default_value = "target/bf-validate")]
        work: PathBuf,
        #[arg(long, default_value = "mod/bf-validator")]
        mod_dir: PathBuf,
        #[arg(long)]
        assets: Option<PathBuf>,
        /// PT: Relatório de saída. EN: Output report.
        #[arg(long, default_value = "validation/measurements/sim-comparison.md")]
        out: PathBuf,
    },
    /// PT: Mede o custo das simulações em blueprints sintéticas. EN: Measures the simulations' cost on synthetic blueprints.
    BenchSim {
        #[arg(long)]
        assets: Option<PathBuf>,
        #[arg(long, default_value = "target/bench")]
        out_dir: PathBuf,
        #[arg(long, default_value = "validation/measurements/sim-benchmarks.md")]
        report: PathBuf,
    },
    /// PT: Calibra a simulação por tick contra a vazão medida. EN: Calibrates the tick simulation against the measured throughput.
    CalibrateTick {
        #[arg(long)]
        assets: Option<PathBuf>,
        #[arg(
            long,
            default_value = "validation/measurements/inserter-throughput.json"
        )]
        table: PathBuf,
    },
    /// PT: Mede a vazão dos insersores no jogo. EN: Measures inserter throughput in-game.
    MeasureInserters {
        #[arg(long)]
        headless: Option<PathBuf>,
        #[arg(long, default_value = "target/bf-validate")]
        work: PathBuf,
        #[arg(long, default_value = "mod/bf-validator")]
        mod_dir: PathBuf,
        /// PT: Tabela de saída. EN: Output table.
        #[arg(
            long,
            default_value = "validation/measurements/inserter-throughput.json"
        )]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        CommandKind::Run {
            scenario,
            headless,
            work,
            mod_dir,
            assets,
        } => {
            let assets = assets
                .unwrap_or_else(|| PathBuf::from(format!("game-assets/{}", bf_data::GAME_VERSION)));
            let passed = run(&scenario, headless, &work, &mod_dir, &assets)?;
            if !passed {
                std::process::exit(1);
            }
            Ok(())
        }
        CommandKind::CompareSim {
            dirs,
            headless,
            work,
            mod_dir,
            assets,
            out,
        } => {
            let headless = match headless {
                Some(path) => path,
                None => default_headless()?,
            };
            let assets = assets
                .unwrap_or_else(|| PathBuf::from(format!("game-assets/{}", bf_data::GAME_VERSION)));
            let simulators: Vec<simcompare::Simulator> =
                vec![("steady", steady_sim), ("tick", tick_sim)];
            let rows = simcompare::run(
                &dirs,
                &headless,
                &work,
                &mod_dir,
                &assets,
                &out,
                &simulators,
            )?;
            if !simcompare::passes(&rows) {
                std::process::exit(1);
            }
            Ok(())
        }
        CommandKind::BenchSim {
            assets,
            out_dir,
            report,
        } => {
            let assets = assets
                .unwrap_or_else(|| PathBuf::from(format!("game-assets/{}", bf_data::GAME_VERSION)));
            bench::run(&assets, &out_dir, &report)
        }
        CommandKind::CalibrateTick { assets, table } => {
            let assets = assets
                .unwrap_or_else(|| PathBuf::from(format!("game-assets/{}", bf_data::GAME_VERSION)));
            calibrate::run(&assets, &table)
        }
        CommandKind::MeasureInserters {
            headless,
            work,
            mod_dir,
            out,
        } => {
            let headless = match headless {
                Some(path) => path,
                None => default_headless()?,
            };
            throughput::measure(&headless, &work, &mod_dir, &out)?;
            Ok(())
        }
    }
}

fn steady_sim(
    input: &bf_sim::SimInput<'_>,
    _warmup: u32,
    _measure: u32,
) -> Result<bf_sim::FlowReport, bf_sim::SimError> {
    bf_sim::steady::simulate(input)
}

// PT: A simulação por tick usa a mesma janela de aquecimento e medição do cenário no jogo.
// EN: The tick simulation uses the same warm-up and measurement window as the in-game scenario.
fn tick_sim(
    input: &bf_sim::SimInput<'_>,
    warmup: u32,
    measure: u32,
) -> Result<bf_sim::FlowReport, bf_sim::SimError> {
    let options = bf_sim::tick::TickOptions {
        warmup_ticks: warmup,
        measure_ticks: measure,
        ..bf_sim::tick::TickOptions::default()
    };
    bf_sim::tick::simulate_with(input, &options, &mut |_| true)
}

fn default_headless() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME ausente / missing HOME")?;
    Ok(PathBuf::from(home).join(format!(
        ".local/opt/factorio-headless-{}",
        bf_data::GAME_VERSION
    )))
}

fn run(
    scenario_path: &Path,
    headless: Option<PathBuf>,
    work: &Path,
    mod_dir: &Path,
    assets: &Path,
) -> Result<bool> {
    let text = fs::read_to_string(scenario_path)
        .with_context(|| format!("falha ao ler / failed to read {}", scenario_path.display()))?;
    // PT: Os cenários ficam em `validation/scenarios/`; os arquivos citados são relativos à pasta
    //     acima da do cenário (`validation/`).
    // EN: Scenarios live in `validation/scenarios/`; the files they cite are relative to the folder
    //     above the scenario's one (`validation/`).
    let validation_dir = scenario_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let scenario = scenario::resolve_file(serde_json::from_str(&text)?, validation_dir)?;
    let prepared = scenario::prepare(scenario)?;
    let headless = match headless {
        Some(path) => path,
        None => default_headless()?,
    };
    let result = execute(&prepared, &headless, work, mod_dir)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    let mut passed = compare(&prepared.scenario, &result);
    if prepared.scenario.compare {
        passed &= compare::with_calculation(&prepared, &result, assets)?;
    }
    Ok(passed)
}

/// PT: Roda um cenário já conferido no headless e devolve o JSON gravado pelo mod.
/// EN: Runs an already checked scenario on the headless server and returns the JSON written by the mod.
pub(crate) fn execute(
    prepared: &scenario::Prepared,
    headless: &Path,
    work: &Path,
    mod_dir: &Path,
) -> Result<Value> {
    let lua = scenario::to_lua(prepared)?;
    let executable = headless.join("bin/x64/factorio");
    if !executable.is_file() {
        bail!(
            "headless ausente; rode scripts/fetch-headless.sh / headless missing; run scripts/fetch-headless.sh"
        );
    }

    // PT: Instância isolada: config, mods e saves próprios dentro da pasta de trabalho.
    // EN: Isolated instance: its own config, mods and saves inside the work folder.
    let instance = work.join(&prepared.scenario.id);
    prepare_owned_dir(&instance, ".bf-validate-instance")?;
    fs::create_dir_all(instance.join("mods"))?;
    fs::create_dir_all(instance.join("saves"))?;
    let instance = fs::canonicalize(&instance)?;
    let write_data = instance.join("write");
    fs::create_dir_all(&write_data)?;
    fs::write(
        instance.join("config.ini"),
        format!(
            "[path]\nread-data={}\nwrite-data={}\n",
            headless.join("data").display(),
            write_data.display()
        ),
    )?;
    let mut mods: Vec<serde_json::Value> = bf_data::OFFICIAL_MODS
        .iter()
        .map(|name| serde_json::json!({ "name": name, "enabled": *name == "base" }))
        .collect();
    mods.push(serde_json::json!({ "name": "bf-validator", "enabled": true }));
    let mod_list = serde_json::json!({ "mods": mods });
    fs::write(
        instance.join("mods/mod-list.json"),
        serde_json::to_vec_pretty(&mod_list)?,
    )?;
    copy_dir(mod_dir, &instance.join("mods/bf-validator"))?;
    fs::write(instance.join("mods/bf-validator/tests/current.lua"), lua)?;
    fs::write(
        instance.join("map-gen-settings.json"),
        r#"{"width": 64, "height": 64, "peaceful_mode": true}"#,
    )?;

    let save = instance.join("saves/bf.zip");
    let config = instance.join("config.ini");
    let mods = instance.join("mods");
    let base_args = |command: &mut Command| {
        command
            .arg("--config")
            .arg(&config)
            .arg("--mod-directory")
            .arg(&mods)
            .current_dir(&instance)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    };

    println!("criando mapa / creating map...");
    let mut create = Command::new(&executable);
    base_args(&mut create);
    create
        .arg("--create")
        .arg(&save)
        .arg("--map-gen-settings")
        .arg(instance.join("map-gen-settings.json"));
    run_with_timeout(create, Duration::from_secs(300), "--create")?;

    let ticks = prepared.scenario.warmup_ticks + prepared.scenario.measure_ticks + 10;
    println!("rodando {ticks} ticks / running {ticks} ticks...");
    let mut bench = Command::new(&executable);
    base_args(&mut bench);
    bench
        .arg("--benchmark")
        .arg(&save)
        .arg("--benchmark-ticks")
        .arg(ticks.to_string())
        .arg("--benchmark-runs")
        .arg("1");
    run_with_timeout(bench, Duration::from_secs(900), "--benchmark")?;

    let result_path = write_data.join(format!(
        "script-output/bf-validator/{}.json",
        prepared.scenario.id
    ));
    let result: Value =
        serde_json::from_str(&fs::read_to_string(&result_path).with_context(|| {
            format!(
                "resultado ausente / result missing: {}",
                result_path.display()
            )
        })?)?;
    Ok(result)
}

// PT: Compara as taxas medidas com as esperadas e imprime o veredito de cada item.
// EN: Compares the measured rates with the expected ones and prints each item's verdict.
fn compare(scenario: &scenario::Scenario, result: &Value) -> bool {
    if result.get("ok") != Some(&Value::Bool(true)) {
        println!(
            "FALHA / FAIL: {}",
            result.get("error").unwrap_or(&Value::Null)
        );
        return false;
    }
    let mut passed = true;
    for (item, expectation) in &scenario.expect {
        // PT: `item` já foi validado como `[a-z0-9-]`, então é seguro no ponteiro JSON.
        // EN: `item` was already validated as `[a-z0-9-]`, so it is safe in the JSON pointer.
        let measured = result
            .pointer(&format!("/rates/{item}/produced_per_second"))
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN);
        let error = (measured - expectation.produced_per_second).abs()
            / expectation.produced_per_second.abs().max(1e-9);
        let ok = error <= expectation.tolerance;
        passed &= ok;
        println!(
            "{} {item}: medido/measured {measured:.4}/s, esperado/expected {:.4}/s, erro/error {:.2}%",
            if ok { "OK   " } else { "FALHA" },
            expectation.produced_per_second,
            error * 100.0
        );
    }
    passed
}

fn run_with_timeout(mut command: Command, timeout: Duration, label: &str) -> Result<()> {
    let mut child = command
        .spawn()
        .with_context(|| format!("falha ao iniciar / failed to start {label}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("{label} terminou com erro / failed: {status}");
            }
            return Ok(());
        }
        if started.elapsed() > timeout {
            // PT: O processo pode ter acabado entre as checagens. EN: The process may have ended between checks.
            let _ = child.kill();
            let _ = child.wait();
            bail!("tempo esgotado em {label} / timed out on {label}");
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// PT: Prepara uma pasta que a ferramenta pode apagar: só limpa se ela tiver o arquivo marcador
///     criado pela própria ferramenta (ou se estiver vazia). Qualquer outra pasta é recusada, para
///     que `--work .` ou `--work ~` nunca apaguem nada do usuário.
/// EN: Prepares a folder the tool may delete: it is only cleared if it holds the marker file the
///     tool itself created (or if it is empty). Any other folder is refused, so that `--work .` or
///     `--work ~` never delete anything of the user's.
pub fn prepare_owned_dir(dir: &Path, marker: &str) -> Result<()> {
    if dir.exists() {
        let owned = dir.join(marker).is_file();
        let empty = fs::read_dir(dir)?.next().is_none();
        if !owned && !empty {
            bail!(
                "recusado: {} não foi criada por esta ferramenta / refused: {} was not created by this tool",
                dir.display(),
                dir.display()
            );
        }
        if owned {
            fs::remove_dir_all(dir)
                .with_context(|| format!("falha ao limpar / failed to clear {}", dir.display()))?;
        }
    }
    fs::create_dir_all(dir)?;
    fs::write(dir.join(marker), b"")?;
    Ok(())
}

// PT: Copia a pasta do mod (sem seguir links simbólicos).
// EN: Copies the mod folder (without following symbolic links).
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
