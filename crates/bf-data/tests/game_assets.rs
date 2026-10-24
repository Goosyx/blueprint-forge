//! PT: Testes com o pacote real gerado pelo `bf-dump`. Sem o pacote eles são pulados, a não ser
//!     que `BF_REQUIRE_GAME_ASSETS=1` (o CI local exige).
//! EN: Tests against the real package built by `bf-dump`. Without the package they are skipped,
//!     unless `BF_REQUIRE_GAME_ASSETS=1` (local CI requires it).

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;

use bf_data::{EntityKind, GameData, Locale, Scope};

fn assets_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("BF_GAME_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../game-assets")
                .join(bf_data::GAME_VERSION)
        });
    if dir.join("gamedata.json").is_file() {
        return Some(dir);
    }
    assert!(
        std::env::var("BF_REQUIRE_GAME_ASSETS").as_deref() != Ok("1"),
        "pacote do jogo ausente / game package missing: {}",
        dir.display()
    );
    eprintln!("pulado: pacote do jogo ausente / skipped: game package missing");
    None
}

fn load() -> Option<GameData> {
    let dir = assets_dir()?;
    let text = std::fs::read_to_string(dir.join("gamedata.json")).unwrap();
    Some(GameData::from_json(&text).unwrap())
}

#[test]
fn spot_values_match_the_game() {
    let Some(data) = load() else { return };

    let gear = &data.recipes["iron-gear-wheel"];
    assert_eq!(gear.energy_required, 0.5);
    assert_eq!(gear.ingredients[0].name, "iron-plate");
    assert_eq!(gear.ingredients[0].amount, 2.0);

    let EntityKind::CraftingMachine { crafting_speed, .. } =
        data.entities["assembling-machine-2"].kind
    else {
        panic!("assembling-machine-2 não é máquina / is not a machine");
    };
    assert_eq!(crafting_speed, 0.75);

    let EntityKind::TransportBelt { speed } = data.entities["transport-belt"].kind else {
        panic!("transport-belt não é esteira / is not a belt");
    };
    assert_eq!(speed * 480.0, 15.0);

    let EntityKind::Beacon {
        distribution_effectivity,
        ..
    } = data.entities["beacon"].kind
    else {
        panic!("beacon não é beacon / is not a beacon");
    };
    assert_eq!(distribution_effectivity, 1.5);

    let EntityKind::Inserter {
        pickup_position, ..
    } = data.entities["inserter"].kind
    else {
        panic!("inserter não é insersor / is not an inserter");
    };
    assert_eq!(pickup_position, [0.0, -1.0]);
}

#[test]
fn science_scope_closes_on_basic_inputs() {
    let Some(data) = load() else { return };
    let scope = Scope::science_1_to_3(&data).unwrap();

    for product in [
        "automation-science-pack",
        "logistic-science-pack",
        "chemical-science-pack",
        "iron-gear-wheel",
        "electronic-circuit",
        "copper-cable",
        "inserter",
        "transport-belt",
        "advanced-circuit",
        "plastic-bar",
        "engine-unit",
        "pipe",
        "sulfur",
    ] {
        assert!(
            scope.recipe_for.contains_key(product),
            "faltou / missing {product}"
        );
    }
    assert!(!scope.recipe_for.contains_key("iron-plate"));
    assert!(!scope.recipe_for.contains_key("steel-plate"));
}

#[test]
fn locales_have_translated_names() {
    let Some(dir) = assets_dir() else { return };
    let read = |language: &str| -> Locale {
        let text = std::fs::read_to_string(dir.join(format!("locale/{language}.json"))).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let en = read("en");
    let pt = read("pt-BR");
    assert_eq!(en.resource("transport-belt"), "Transport belt");
    assert_eq!(pt.resource("transport-belt"), "Esteira");
    assert_ne!(
        pt.recipe("automation-science-pack"),
        "automation-science-pack"
    );
}
