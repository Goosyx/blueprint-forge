//! PT: Converte os arquivos `*-locale.json` do `--dump-prototype-locale` num `bf_data::Locale`,
//!     mantendo só os nomes dos protótipos que existem no `gamedata.json`.
//! EN: Converts the `*-locale.json` files from `--dump-prototype-locale` into a `bf_data::Locale`,
//!     keeping only the names of prototypes that exist in `gamedata.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use bf_data::{GameData, Locale};
use serde::Deserialize;

#[derive(Deserialize)]
struct LocaleFile {
    #[serde(default)]
    names: BTreeMap<String, String>,
}

/// PT: Lê os nomes de `dir` (a `script-output` do jogo) para o idioma `language`.
/// EN: Reads the names from `dir` (the game's `script-output`) for the `language` language.
pub fn read(dir: &Path, language: &str, data: &GameData) -> Result<Locale> {
    let load = |file: &str| -> Result<BTreeMap<String, String>> {
        let path = dir.join(file);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("falha ao ler / failed to read {}", path.display()))?;
        let parsed: LocaleFile = serde_json::from_str(&text)
            .with_context(|| format!("JSON inválido / invalid JSON in {}", path.display()))?;
        Ok(parsed.names)
    };

    let keep = |names: BTreeMap<String, String>, known: &dyn Fn(&str) -> bool| {
        names
            .into_iter()
            .filter(|(name, text)| known(name) && !text.is_empty())
            .collect::<BTreeMap<_, _>>()
    };

    Ok(Locale {
        language: language.to_owned(),
        items: keep(load("item-locale.json")?, &|name| {
            data.items.contains_key(name)
        }),
        fluids: keep(load("fluid-locale.json")?, &|name| {
            data.fluids.contains_key(name)
        }),
        recipes: keep(load("recipe-locale.json")?, &|name| {
            data.recipes.contains_key(name)
        }),
        entities: keep(load("entity-locale.json")?, &|name| {
            data.entities.contains_key(name)
        }),
        virtual_signals: keep(load("virtual-signal-locale.json")?, &|_| true),
    })
}
