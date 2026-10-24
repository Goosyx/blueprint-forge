//! PT: Análise completa das blueprints de referência exportadas do jogo (`validation/golden`)
//!     com os dados reais. Pulada sem o pacote, salvo `BF_REQUIRE_GAME_ASSETS=1`; as blueprints
//!     fazem parte do repositório e são sempre obrigatórias.
//! EN: Full analysis of the reference blueprints exported from the game (`validation/golden`)
//!     with the real data. Skipped without the package, unless `BF_REQUIRE_GAME_ASSETS=1`; the
//!     blueprints are part of the repository and always required.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::PathBuf;

use bf_analysis::{Analysis, analyze};
use bf_blueprint::{BlueprintString, Limits, decode};
use bf_calc::AnalysisOptions;
use bf_data::GameData;

fn load() -> Option<GameData> {
    let dir = std::env::var_os("BF_GAME_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../game-assets")
                .join(bf_data::GAME_VERSION)
        });
    let path = dir.join("gamedata.json");
    if !path.is_file() {
        assert!(
            std::env::var("BF_REQUIRE_GAME_ASSETS").as_deref() != Ok("1"),
            "pacote ausente / package missing"
        );
        return None;
    }
    Some(GameData::from_json(&std::fs::read_to_string(path).unwrap()).unwrap())
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../validation/golden")
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(golden_dir().join(name)).expect(name)
}

fn run(data: &GameData, text: &str) -> Analysis {
    analyze(
        data,
        text,
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn every_golden_is_fully_understood() {
    let Some(data) = load() else { return };
    let mut checked = 0;
    for entry in std::fs::read_dir(golden_dir())
        .unwrap()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "txt") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let analysis = run(&data, &std::fs::read_to_string(&path).unwrap());
        // PT: Nenhuma entidade desconhecida, sobreposta ou fora da grade.
        // EN: No unknown, overlapping or off-grid entity.
        assert!(
            analysis.world.warnings.is_empty() && analysis.world.dropped_warnings == 0,
            "{name}: {:?}",
            analysis.world.warnings
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "nenhuma blueprint de referência / no reference blueprint"
    );
}

// PT: A versão gravada nas blueprints geradas é a mesma versão do jogo dos dados.
// EN: The version written into generated blueprints is the same game version as the data.
#[test]
fn blueprint_version_matches_the_game_version() {
    let parts: Vec<u64> = bf_data::GAME_VERSION
        .split('.')
        .map(|part| part.parse().unwrap())
        .collect();
    let packed = bf_blueprint::GAME_VERSION;
    assert_eq!(
        vec![
            packed >> 48,
            (packed >> 32) & 0xffff,
            (packed >> 16) & 0xffff
        ],
        parts
    );
}

// PT: Alcance do beacon (3 tiles a partir da borda): conferido caso a caso no `02-beacons`.
// EN: Beacon reach (3 tiles from the edge): checked case by case in `02-beacons`.
#[test]
fn beacon_reach_matches_the_golden_layout() {
    let Some(data) = load() else { return };
    let text = golden("02-beacons.txt");
    let analysis = run(&data, &text);
    let BlueprintString::Blueprint(blueprint) = decode(&text, &Limits::default()).unwrap().content
    else {
        panic!("não é blueprint / not a blueprint");
    };
    let beacons_at = |x: f64, y: f64| -> usize {
        let number = blueprint
            .entities
            .iter()
            .find(|e| e.position.x == x && e.position.y == y)
            .unwrap()
            .entity_number;
        analysis
            .report
            .machines
            .iter()
            .find(|m| m.entity_number == number)
            .unwrap()
            .beacons
    };
    // PT: 2 tiles livres pega; 3 não pega; diagonal 2×2 pega.
    // EN: 2 free tiles reach; 3 do not; 2×2 diagonal reaches.
    assert_eq!(beacons_at(39.5, -81.5), 1);
    assert_eq!(beacons_at(48.5, -81.5), 0);
    assert_eq!(beacons_at(9.5, -79.5), 1);
    // PT: Linha de 5 montadoras entre duas fileiras de beacons: 4, 6, 6, 6, 4.
    // EN: Row of 5 assemblers between two beacon rows: 4, 6, 6, 6, 4.
    let row: Vec<usize> = [82.5, 85.5, 88.5, 91.5, 94.5]
        .iter()
        .map(|x| beacons_at(*x, -81.5))
        .collect();
    assert_eq!(row, vec![4, 6, 6, 6, 4]);
    // PT: Beacon vazio ao lado de um cheio. EN: Empty beacon next to a full one.
    assert_eq!(beacons_at(39.5, -71.5), 2);
}
