//! PT: Nomes traduzidos dos protótipos, no formato dos arquivos `locale/<idioma>.json`.
//! EN: Translated prototype names, in the format of the `locale/<language>.json` files.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// PT: Nomes de exibição de um idioma. Nomes ausentes caem no nome interno.
/// EN: Display names for one language. Missing names fall back to the internal name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Locale {
    /// PT: Código do idioma, por exemplo `"en"` ou `"pt-BR"`.
    /// EN: Language code, for example `"en"` or `"pt-BR"`.
    pub language: String,
    #[serde(default)]
    pub items: BTreeMap<String, String>,
    #[serde(default)]
    pub fluids: BTreeMap<String, String>,
    #[serde(default)]
    pub recipes: BTreeMap<String, String>,
    #[serde(default)]
    pub entities: BTreeMap<String, String>,
    #[serde(default)]
    pub virtual_signals: BTreeMap<String, String>,
}

impl Locale {
    /// PT: Nome de exibição de um item ou fluido; se não houver tradução, devolve o nome interno.
    /// EN: Display name of an item or fluid; without a translation, returns the internal name.
    pub fn resource<'a>(&'a self, name: &'a str) -> &'a str {
        self.items
            .get(name)
            .or_else(|| self.fluids.get(name))
            .map_or(name, String::as_str)
    }

    /// PT: Nome de exibição de uma receita; cai no nome do item de mesmo nome e depois no nome interno.
    /// EN: Display name of a recipe; falls back to the same-named item and then to the internal name.
    pub fn recipe<'a>(&'a self, name: &'a str) -> &'a str {
        self.recipes
            .get(name)
            .map_or_else(|| self.resource(name), String::as_str)
    }

    /// PT: Nome de exibição de uma entidade; se não houver tradução, devolve o nome interno.
    /// EN: Display name of an entity; without a translation, returns the internal name.
    pub fn entity<'a>(&'a self, name: &'a str) -> &'a str {
        self.entities.get(name).map_or(name, String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_internal_names() {
        let mut locale = Locale {
            language: "pt-BR".into(),
            ..Locale::default()
        };
        locale
            .items
            .insert("iron-plate".into(), "Placa de ferro".into());

        assert_eq!(locale.resource("iron-plate"), "Placa de ferro");
        assert_eq!(locale.recipe("iron-plate"), "Placa de ferro");
        assert_eq!(locale.resource("unknown"), "unknown");
        assert_eq!(locale.entity("unknown"), "unknown");
    }
}
