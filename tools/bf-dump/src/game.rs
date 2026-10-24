//! PT: Localiza a instalação oficial do jogo e executa os comandos de exportação
//!     (`--dump-data`, `--dump-prototype-locale`, `--dump-icon-sprites`) numa pasta isolada,
//!     sem tocar no `~/.factorio` do usuário.
//! EN: Locates the official game installation and runs the export commands
//!     (`--dump-data`, `--dump-prototype-locale`, `--dump-icon-sprites`) in an isolated folder,
//!     without touching the user's `~/.factorio`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// PT: Id do Factorio na Steam. Com ele definido, a versão da Steam roda direto,
///     sem pedir para ser reiniciada pelo cliente.
/// EN: Factorio's Steam app id. When it is set, the Steam build runs directly
///     instead of asking the client to restart it.
const STEAM_APP_ID: &str = "427520";

/// PT: Instalação do jogo encontrada e conferida.
/// EN: Game installation found and verified.
#[derive(Debug, Clone)]
pub struct Game {
    pub root: PathBuf,
    pub executable: PathBuf,
    /// PT: Primeira linha de `--version`, por exemplo `Version: 2.1.23 (build 87750, linux64, steam)`.
    /// EN: First line of `--version`, for example `Version: 2.1.23 (build 87750, linux64, steam)`.
    pub version_line: String,
    pub version: String,
    pub build: u32,
    pub is_steam: bool,
    /// PT: Servidor headless: sem áudio e sem gráficos (não exporta ícones).
    /// EN: Headless server: no audio and no graphics (cannot export icons).
    pub is_headless: bool,
}

/// PT: Procura o jogo em `explicit` (ou `FACTORIO_PATH`) e depois nos caminhos padrão da Steam.
/// EN: Looks for the game in `explicit` (or `FACTORIO_PATH`) and then in the default Steam paths.
pub fn locate(explicit: Option<&Path>) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit {
        candidates.push(path.to_owned());
    } else if let Some(path) = std::env::var_os("FACTORIO_PATH") {
        candidates.push(PathBuf::from(path));
    } else if let Some(home) = &home {
        candidates.push(home.join(".local/share/Steam/steamapps/common/Factorio"));
        candidates.push(home.join(".steam/steam/steamapps/common/Factorio"));
    }

    for root in candidates {
        if executable_of(&root).is_file() {
            return Ok(root);
        }
    }
    bail!(
        "jogo não encontrado; use --game ou FACTORIO_PATH / game not found; use --game or FACTORIO_PATH"
    )
}

fn executable_of(root: &Path) -> PathBuf {
    root.join("bin/x64/factorio")
}

/// PT: Confere que é um build oficial `linux64` na versão pedida.
/// EN: Checks that it is an official `linux64` build of the requested version.
pub fn inspect(root: &Path, required_version: &str) -> Result<Game> {
    let executable = executable_of(root);
    let output = Command::new(&executable)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("falha ao executar / failed to run {}", executable.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout.lines().next().unwrap_or_default().trim().to_owned();

    let (version, build, platform, distribution) = parse_version_line(&version_line)
        .with_context(|| format!("saída inesperada / unexpected output: {version_line:?}"))?;
    if platform != "linux64" {
        bail!("build não suportado / unsupported build: {version_line}");
    }
    if version != required_version {
        bail!(
            "versão {version}, esperada {required_version} / version {version}, expected {required_version}"
        );
    }
    Ok(Game {
        root: root.to_owned(),
        executable,
        version_line,
        version,
        build,
        is_steam: distribution == "steam",
        is_headless: distribution == "headless",
    })
}

// PT: Lê `Version: 2.1.23 (build 87750, linux64, steam)` → (versão, build, plataforma, distribuição).
// EN: Parses `Version: 2.1.23 (build 87750, linux64, steam)` → (version, build, platform, distribution).
fn parse_version_line(line: &str) -> Option<(String, u32, String, String)> {
    let rest = line.strip_prefix("Version: ")?;
    let (version, details) = rest.split_once(' ')?;
    let details = details.strip_prefix('(')?.strip_suffix(')')?;
    let mut parts = details.split(',').map(str::trim);
    let build = parts.next()?.strip_prefix("build ")?.parse().ok()?;
    let platform = parts.next()?.to_owned();
    let distribution = parts.next()?.to_owned();
    Some((version.to_owned(), build, platform, distribution))
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

/// PT: Pasta de trabalho isolada: config própria, write-data próprio e lista de mods só com o base.
/// EN: Isolated work folder: its own config, its own write-data and a mod list with only base.
pub struct Workspace {
    pub dir: PathBuf,
    config: PathBuf,
    mods: PathBuf,
}

impl Workspace {
    /// PT: Cria (ou recria) a pasta de trabalho em `dir`.
    /// EN: Creates (or recreates) the work folder at `dir`.
    pub fn create(dir: &Path) -> Result<Self> {
        prepare_owned_dir(dir, ".bf-dump-workspace")?;
        // PT: Caminho absoluto, porque o jogo roda com a própria pasta de trabalho como diretório atual.
        // EN: Absolute path, because the game runs with the work folder as its current directory.
        let dir = fs::canonicalize(dir)?;
        let mods = dir.join("mods");
        fs::create_dir_all(&mods)?;

        let mut mod_list = Vec::new();
        for name in bf_data::OFFICIAL_MODS {
            mod_list.push(serde_json::json!({ "name": name, "enabled": *name == "base" }));
        }
        fs::write(
            mods.join("mod-list.json"),
            serde_json::to_vec_pretty(&serde_json::json!({ "mods": mod_list }))?,
        )?;

        Ok(Self {
            config: dir.join("config.ini"),
            dir,
            mods,
        })
    }

    /// PT: Pasta onde o jogo grava as exportações.
    /// EN: Folder where the game writes its exports.
    pub fn script_output(&self) -> PathBuf {
        self.dir.join("write/script-output")
    }

    // PT: Escreve o config.ini apontando para os dados do jogo e para o write-data isolado.
    // EN: Writes config.ini pointing at the game data and at the isolated write-data.
    fn write_config(&self, game: &Game, locale: &str) -> Result<()> {
        let read_data = game.root.join("data");
        let write_data = self.dir.join("write");
        fs::create_dir_all(&write_data)?;
        let text = format!(
            "[path]\nread-data={}\nwrite-data={}\n[general]\nlocale={}\n",
            read_data.display(),
            write_data.display(),
            locale
        );
        fs::write(&self.config, text)?;
        Ok(())
    }

    /// PT: Executa o jogo com uma flag de exportação e espera no máximo `timeout`.
    /// EN: Runs the game with one export flag and waits at most `timeout`.
    pub fn run(&self, game: &Game, flag: &str, locale: &str, timeout: Duration) -> Result<()> {
        self.write_config(game, locale)?;
        let mut command = Command::new(&game.executable);
        command
            .arg("--config")
            .arg(&self.config)
            .arg("--mod-directory")
            .arg(&self.mods)
            .arg(flag)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // PT: O headless recusa `--disable-audio`. EN: The headless server rejects `--disable-audio`.
        if !game.is_headless {
            command.arg("--disable-audio");
        }
        if game.is_steam {
            command
                .env("SteamAppId", STEAM_APP_ID)
                .env("SteamGameId", STEAM_APP_ID);
        }

        let mut child = command
            .spawn()
            .context("falha ao iniciar o jogo / failed to start the game")?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    bail!(
                        "o jogo terminou com erro em {flag} / the game failed on {flag}: {status}"
                    );
                }
                return Ok(());
            }
            if started.elapsed() > timeout {
                // PT: Ignora erro do kill: o processo pode ter acabado entre as duas checagens.
                // EN: Ignores the kill error: the process may have ended between the two checks.
                let _ = child.kill();
                let _ = child.wait();
                bail!("tempo esgotado em {flag} / timed out on {flag}");
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_version_lines() {
        let parsed = parse_version_line("Version: 2.1.23 (build 87750, linux64, steam)");
        assert_eq!(
            parsed,
            Some(("2.1.23".into(), 87750, "linux64".into(), "steam".into()))
        );
        let headless = parse_version_line("Version: 2.1.23 (build 87750, linux64, headless)");
        assert_eq!(headless.map(|v| v.3), Some("headless".into()));
        assert_eq!(parse_version_line("garbage"), None);
        assert_eq!(
            parse_version_line("Version: 2.1.23 (build x, linux64, steam)"),
            None
        );
    }
}
