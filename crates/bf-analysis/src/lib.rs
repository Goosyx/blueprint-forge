//! PT: Fachada única do pipeline de análise. O navegador (WASM) e o servidor chamam as mesmas
//!     funções, o que garante o mesmo resultado nos dois lados. Só a blueprint escolhida ganha
//!     mundo e cálculo; as outras de um livro ficam só com o resumo, para que o teto de tiles
//!     valha para a análise inteira e não por blueprint.
//! EN: Single facade of the analysis pipeline. The browser (WASM) and the server call the same
//!     functions, which guarantees the same result on both sides. Only the chosen blueprint gets
//!     a world and a calculation; the others in a book only get a summary, so the tile cap applies
//!     to the whole analysis rather than per blueprint.

use bf_blueprint::{
    Blueprint, BlueprintBook, BlueprintString, DecodeError, Limits, Warning, decode,
};
use bf_calc::{AnalysisOptions, CalcError, CalcReport, calculate};
use bf_data::GameData;
use bf_world::{World, WorldError};
use serde::Serialize;

/// PT: Erro da análise, com código estável para tradução.
/// EN: Analysis error, with a stable code for translation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    World(#[from] WorldError),
    #[error(transparent)]
    Calc(#[from] CalcError),
    #[error("opções inválidas / invalid options: {0}")]
    Options(&'static str),
    #[error("blueprint inexistente no livro / blueprint not found in the book: {0}")]
    NoSuchBlueprint(usize),
}

impl AnalysisError {
    /// PT: Código estável do erro, usado como chave de tradução.
    /// EN: Stable error code, used as the translation key.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Decode(error) => error.code(),
            Self::World(error) => error.code(),
            Self::Calc(error) => error.code(),
            Self::Options(code) => code,
            Self::NoSuchBlueprint(_) => "analysis.no_such_blueprint",
        }
    }
}

/// PT: Ícone de uma blueprint ou livro.
/// EN: Icon of a blueprint or book.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IconRef {
    /// PT: `item`, `fluid`, `virtual`, `entity` ou `recipe` (o jogo omite `item`).
    /// EN: `item`, `fluid`, `virtual`, `entity` or `recipe` (the game omits `item`).
    pub kind: String,
    pub name: String,
}

/// PT: Nó da árvore de um livro, para a navegação na interface.
/// EN: Book tree node, for navigation in the interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TreeNode {
    Book {
        label: Option<String>,
        icons: Vec<IconRef>,
        children: Vec<TreeNode>,
    },
    /// PT: `blueprint` é a posição em `Analysis::blueprints`.
    /// EN: `blueprint` is the position in `Analysis::blueprints`.
    Blueprint { blueprint: usize },
    /// PT: Planejador de upgrade ou desconstrução, mantido mas não analisado.
    /// EN: Upgrade or deconstruction planner, kept but not analyzed.
    Other { kind: String },
}

/// PT: Resumo de uma blueprint (sem mundo nem cálculo).
/// EN: Summary of one blueprint (no world or calculation).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlueprintSummary {
    pub label: Option<String>,
    pub description: Option<String>,
    pub icons: Vec<IconRef>,
    pub entity_count: usize,
}

/// PT: Resultado da análise: árvore, resumos, e mundo e cálculo da blueprint `selected`.
/// EN: Analysis result: tree, summaries, and world and calculation of blueprint `selected`.
#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub tree: TreeNode,
    pub blueprints: Vec<BlueprintSummary>,
    /// PT: Códigos dos avisos de leitura (por exemplo, `converted_from_legacy`).
    /// EN: Codes of the reading warnings (for example, `converted_from_legacy`).
    pub warnings: Vec<String>,
    pub selected: usize,
    pub world: World,
    pub report: CalcReport,
}

/// PT: Decodifica, monta o mundo da blueprint escolhida e calcula com as opções do usuário.
/// EN: Decodes, builds the chosen blueprint's world and calculates with the user's options.
pub fn analyze(
    data: &GameData,
    text: &str,
    selected: usize,
    options: &AnalysisOptions,
    limits: &Limits,
) -> Result<Analysis, AnalysisError> {
    options.validate().map_err(AnalysisError::Options)?;
    let decoded = decode(text, limits)?;
    let warnings = decoded
        .warnings
        .iter()
        .map(|warning| match warning {
            Warning::ConvertedFromLegacy => "converted_from_legacy".to_owned(),
        })
        .collect();

    let mut ordered = Vec::new();
    let tree = match &decoded.content {
        BlueprintString::Blueprint(blueprint) => {
            ordered.push(blueprint);
            TreeNode::Blueprint { blueprint: 0 }
        }
        BlueprintString::Book(book) => book_tree(book, &mut ordered),
    };
    let blueprint = ordered
        .get(selected)
        .copied()
        .ok_or(AnalysisError::NoSuchBlueprint(selected))?;
    let world = World::build(blueprint, data)?;
    let report = calculate(data, blueprint, &world, options)?;

    Ok(Analysis {
        tree,
        blueprints: ordered.iter().map(|b| summary(b)).collect(),
        warnings,
        selected,
        world,
        report,
    })
}

fn summary(blueprint: &Blueprint) -> BlueprintSummary {
    BlueprintSummary {
        label: blueprint.label.clone(),
        description: blueprint.description.clone(),
        icons: icons(&blueprint.icons),
        entity_count: blueprint.entities.len(),
    }
}

// PT: Monta a árvore do livro e a lista de blueprints na mesma ordem (índice, em profundidade).
//     A profundidade já foi limitada pelo codec (`Limits::max_book_depth`).
// EN: Builds the book tree and the blueprint list in the same order (index, depth first).
//     Depth was already limited by the codec (`Limits::max_book_depth`).
fn book_tree<'a>(book: &'a BlueprintBook, ordered: &mut Vec<&'a Blueprint>) -> TreeNode {
    let mut entries: Vec<_> = book.blueprints.iter().collect();
    entries.sort_by_key(|entry| entry.index);

    let mut children = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(blueprint) = &entry.blueprint {
            ordered.push(blueprint);
            children.push(TreeNode::Blueprint {
                blueprint: ordered.len() - 1,
            });
        } else if let Some(child) = &entry.blueprint_book {
            children.push(book_tree(child, ordered));
        } else if entry.upgrade_planner.is_some() {
            children.push(TreeNode::Other {
                kind: "upgrade_planner".to_owned(),
            });
        } else if entry.deconstruction_planner.is_some() {
            children.push(TreeNode::Other {
                kind: "deconstruction_planner".to_owned(),
            });
        }
    }
    TreeNode::Book {
        label: book.label.clone(),
        icons: icons(&book.icons),
        children,
    }
}

fn icons(icons: &[bf_blueprint::Icon]) -> Vec<IconRef> {
    let mut sorted: Vec<_> = icons.iter().collect();
    sorted.sort_by_key(|icon| icon.index);
    sorted
        .into_iter()
        .map(|icon| IconRef {
            kind: icon
                .signal
                .kind
                .clone()
                .unwrap_or_else(|| "item".to_owned()),
            name: icon.signal.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bf_blueprint::{BookEntry, Entity, Position, encode};
    use std::collections::BTreeMap;

    fn empty_data() -> GameData {
        GameData {
            format_version: bf_data::FORMAT_VERSION,
            game_version: "2.1.23".into(),
            items: BTreeMap::new(),
            fluids: BTreeMap::new(),
            recipes: BTreeMap::new(),
            entities: BTreeMap::new(),
            tiles: BTreeMap::new(),
        }
    }

    fn book() -> String {
        let blueprint = |label: &str| Blueprint {
            label: Some(label.into()),
            entities: vec![Entity::new(1, "wooden-chest", Position { x: 0.5, y: 0.5 })],
            ..Blueprint::default()
        };
        let entry = |index: u32, label: &str| BookEntry {
            index,
            blueprint: Some(blueprint(label)),
            blueprint_book: None,
            upgrade_planner: None,
            deconstruction_planner: None,
        };
        encode(&BlueprintString::Book(BlueprintBook {
            item: "blueprint-book".into(),
            label: Some("Livro".into()),
            label_color: None,
            description: None,
            icons: Vec::new(),
            blueprints: vec![entry(1, "segunda"), entry(0, "primeira")],
            active_index: 0,
            version: bf_blueprint::GAME_VERSION,
        }))
        .unwrap()
    }

    #[test]
    fn analyzes_only_the_selected_blueprint_in_index_order() {
        let options = AnalysisOptions::default();
        let analysis = analyze(&empty_data(), &book(), 1, &options, &Limits::default()).unwrap();
        assert_eq!(analysis.blueprints[0].label.as_deref(), Some("primeira"));
        assert_eq!(analysis.blueprints[1].label.as_deref(), Some("segunda"));
        assert_eq!(analysis.selected, 1);
        let json = serde_json::to_value(&analysis).unwrap();
        assert_eq!(json["tree"]["type"], "book");
        assert_eq!(json["tree"]["children"][1]["blueprint"], 1);
        assert_eq!(json["world"]["warnings"][0]["code"], "unknown_entity");
        assert!(json["blueprints"][0].get("world").is_none());
    }

    #[test]
    fn exposes_error_codes() {
        let options = AnalysisOptions::default();
        let error = analyze(
            &empty_data(),
            "not a blueprint",
            0,
            &options,
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), "blueprint.unsupported_version");
        let error = analyze(&empty_data(), &book(), 5, &options, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), "analysis.no_such_blueprint");
    }
}
