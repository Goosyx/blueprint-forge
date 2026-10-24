//! PT: `bf-dump` exporta os dados, os nomes (en e pt-BR) e os ícones da cópia oficial do jogo
//!     instalada na máquina e grava o pacote normalizado em `game-assets/<versão>/`.
//! EN: `bf-dump` exports the data, the names (en and pt-BR) and the icons from the official
//!     copy of the game installed on the machine and writes the normalized package into
//!     `game-assets/<version>/`.

mod game;
mod icons;
mod locale;
mod normalize;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// PT: Idiomas exportados.
/// EN: Exported languages.
const LANGUAGES: &[&str] = &["en", "pt-BR"];

#[derive(Parser)]
#[command(version, about = "Exporta os dados do jogo / Exports the game data")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PT: Roda o jogo e gera o pacote. EN: Runs the game and builds the package.
    Export {
        /// PT: Pasta do jogo (padrão: FACTORIO_PATH ou Steam). EN: Game folder (default: FACTORIO_PATH or Steam).
        #[arg(long)]
        game: Option<PathBuf>,
        /// PT: Versão exigida. EN: Required version.
        #[arg(long, default_value = bf_data::GAME_VERSION)]
        version: String,
        /// PT: Pasta de saída. EN: Output folder.
        #[arg(long, default_value = "game-assets")]
        out: PathBuf,
        /// PT: Pasta de trabalho temporária. EN: Temporary work folder.
        #[arg(long, default_value = "target/bf-dump-work")]
        work: PathBuf,
        /// PT: Pula os ícones (não abre a parte gráfica do jogo). EN: Skips icons (does not start the game graphics).
        #[arg(long)]
        no_icons: bool,
        /// PT: Reaproveita as exportações já coletadas na pasta de trabalho. EN: Reuses exports already collected in the work folder.
        #[arg(long)]
        reuse: bool,
    },
}

/// PT: Conteúdo do `manifest.json` do pacote.
/// EN: Contents of the package's `manifest.json`.
#[derive(Serialize)]
struct Manifest {
    format_version: u32,
    game_version: String,
    build: u32,
    version_line: String,
    source: &'static str,
    has_icons: bool,
    /// PT: Caminho relativo → sha256 de cada arquivo do pacote.
    /// EN: Relative path → sha256 of each package file.
    files: BTreeMap<String, String>,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Export {
            game,
            version,
            out,
            work,
            no_icons,
            reuse,
        } => export(game.as_deref(), &version, &out, &work, !no_icons, reuse),
    }
}

fn export(
    game_dir: Option<&Path>,
    version: &str,
    out: &Path,
    work: &Path,
    with_icons: bool,
    reuse: bool,
) -> Result<()> {
    let root = game::locate(game_dir)?;
    let game = game::inspect(&root, version)?;
    println!(
        "jogo / game: {} ({})",
        game.root.display(),
        game.version_line
    );

    if with_icons && game.is_headless {
        bail!(
            "o headless não exporta ícones; use --no-icons / the headless server cannot export icons; use --no-icons"
        );
    }

    let collected = work.join("collected");
    if !reuse || !collected.join("data-raw-dump.json").is_file() {
        collect(&game, work, &collected, with_icons)?;
    }

    let raw_path = collected.join("data-raw-dump.json");
    let raw_text = fs::read_to_string(&raw_path)
        .with_context(|| format!("falha ao ler / failed to read {}", raw_path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&raw_text)?;
    let data = normalize::normalize(&raw, &game.version)?;
    println!(
        "normalizado / normalized: {} itens/items, {} fluidos/fluids, {} receitas/recipes, {} entidades/entities",
        data.items.len(),
        data.fluids.len(),
        data.recipes.len(),
        data.entities.len()
    );

    // PT: Monta o pacote numa pasta temporária e só troca pelo antigo quando tudo deu certo.
    // EN: Builds the package in a temporary folder and only replaces the old one when everything worked.
    let final_dir = out.join(&game.version);
    let staging = out.join(format!(".{}.staging", game.version));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(staging.join("locale"))?;
    fs::create_dir_all(staging.join("raw"))?;

    fs::write(staging.join("gamedata.json"), serde_json::to_vec(&data)?)?;
    fs::copy(&raw_path, staging.join("raw/data-raw-dump.json"))?;

    let mut virtual_signals = Vec::new();
    for language in LANGUAGES {
        let locale = locale::read(
            &collected.join(format!("locale-{language}")),
            language,
            &data,
        )?;
        if virtual_signals.is_empty() {
            virtual_signals = locale.virtual_signals.keys().cloned().collect();
        }
        fs::write(
            staging.join(format!("locale/{language}.json")),
            serde_json::to_vec(&locale)?,
        )?;
    }

    let icons_dir = collected.join("icons");
    let has_icons = with_icons && icons_dir.is_dir();
    if has_icons {
        let atlas = icons::build(&icons_dir, &data, &virtual_signals, &staging.join("icons"))?;
        println!(
            "ícones / icons: {} em / in {} página(s)/page(s)",
            atlas.icons.len(),
            atlas.pages.len()
        );
        fs::write(
            staging.join("icons/atlas.json"),
            serde_json::to_vec(&atlas)?,
        )?;
    }

    let manifest = Manifest {
        format_version: bf_data::FORMAT_VERSION,
        game_version: game.version.clone(),
        build: game.build,
        version_line: game.version_line.clone(),
        source: "dump",
        has_icons,
        files: hash_files(&staging)?,
    };
    fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    // PT: Troca o pacote antigo pelo novo sem perder o antigo se o rename falhar.
    // EN: Swaps the old package for the new one without losing the old one if the rename fails.
    let backup = out.join(format!(".{}.old", game.version));
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    let had_old = final_dir.exists();
    if had_old {
        fs::rename(&final_dir, &backup)?;
    }
    if let Err(error) = fs::rename(&staging, &final_dir) {
        if had_old {
            fs::rename(&backup, &final_dir)?;
        }
        return Err(error.into());
    }
    if had_old {
        fs::remove_dir_all(&backup)?;
    }
    println!("pacote / package: {}", final_dir.display());
    Ok(())
}

// PT: Roda cada exportação do jogo e move o resultado para `collected`.
// EN: Runs each game export and moves the result into `collected`.
fn collect(game: &game::Game, work: &Path, collected: &Path, with_icons: bool) -> Result<()> {
    let workspace = game::Workspace::create(work)?;
    fs::create_dir_all(collected)?;
    let output = workspace.script_output();

    println!("exportando dados / exporting data...");
    workspace.run(game, "--dump-data", "en", Duration::from_secs(300))?;
    fs::rename(
        output.join("data-raw-dump.json"),
        collected.join("data-raw-dump.json"),
    )?;

    for language in LANGUAGES {
        println!("exportando nomes / exporting names ({language})...");
        clear_dir(&output)?;
        workspace.run(
            game,
            "--dump-prototype-locale",
            language,
            Duration::from_secs(300),
        )?;
        let target = collected.join(format!("locale-{language}"));
        fs::create_dir_all(&target)?;
        for entry in fs::read_dir(&output)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().ends_with("-locale.json") {
                fs::rename(entry.path(), target.join(name))?;
            }
        }
    }

    if with_icons {
        println!(
            "exportando ícones (o jogo abre a janela por alguns segundos) / exporting icons (the game window opens for a few seconds)..."
        );
        clear_dir(&output)?;
        workspace.run(game, "--dump-icon-sprites", "en", Duration::from_secs(900))?;
        let target = collected.join("icons");
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        fs::rename(&output, &target)?;
    }
    Ok(())
}

fn clear_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    Ok(())
}

// PT: sha256 de todos os arquivos de `root`, com caminhos relativos e separador `/`.
// EN: sha256 of every file under `root`, with relative paths and `/` separators.
fn hash_files(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(root)?;
            let Some(relative) = relative.to_str() else {
                bail!("caminho não UTF-8 / non-UTF-8 path: {}", path.display());
            };
            let digest = Sha256::digest(fs::read(&path)?);
            hashes.insert(relative.replace('\\', "/"), hex::encode(digest));
        }
    }
    Ok(hashes)
}
