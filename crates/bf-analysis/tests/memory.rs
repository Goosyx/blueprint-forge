//! PT: Pico de memória de um livro com várias blueprints enormes (achado do code review da Fase 1):
//!     só a blueprint escolhida monta mundo, então o teto de tiles vale para a análise inteira.
//! EN: Peak memory of a book with several huge blueprints (Phase 1 code review finding): only the
//!     chosen blueprint builds a world, so the tile cap applies to the whole analysis.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use bf_analysis::analyze;
use bf_blueprint::{Limits, encode_raw_json};
use bf_calc::AnalysisOptions;
use bf_data::{Entity, EntityKind, GameData, TileSize};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

const MAX_PEAK_BYTES: usize = 160 * 1024 * 1024;

fn data() -> GameData {
    let mut entities = BTreeMap::new();
    entities.insert(
        "rocket-silo".to_owned(),
        Entity {
            name: "rocket-silo".into(),
            prototype: "rocket-silo".into(),
            size: TileSize {
                width: 9,
                height: 9,
            },
            collision_box: [[-4.2, -4.2], [4.2, 4.2]],
            placed_by: Vec::new(),
            energy_source: None,
            energy_usage_w: None,
            fluid_boxes: Vec::new(),
            kind: EntityKind::Other,
        },
    );
    GameData {
        format_version: bf_data::FORMAT_VERSION,
        game_version: "test".into(),
        items: BTreeMap::new(),
        fluids: BTreeMap::new(),
        recipes: BTreeMap::new(),
        entities,
        tiles: BTreeMap::new(),
    }
}

#[test]
fn book_of_huge_blueprints_stays_below_the_cap() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let mut entities = String::new();
    for number in 1..=49_000u64 {
        let x = 4.5 + 9.0 * (number % 220) as f64;
        let y = 4.5 + 9.0 * (number / 220) as f64;
        if number > 1 {
            entities.push(',');
        }
        entities.push_str(&format!(
            r#"{{"entity_number":{number},"name":"rocket-silo","position":{{"x":{x},"y":{y}}}}}"#
        ));
    }
    let blueprint = format!(
        r#"{{"blueprint":{{"item":"blueprint","version":562954249895936,"entities":[{entities}]}}}}"#
    );
    let entries: Vec<String> = (0..4)
        .map(|index| {
            format!(
                r#"{{"index":{index},{}}}"#,
                &blueprint[1..blueprint.len() - 1]
            )
        })
        .collect();
    let book = format!(
        r#"{{"blueprint_book":{{"item":"blueprint-book","version":562954249895936,"blueprints":[{}]}}}}"#,
        entries.join(",")
    );
    let text = encode_raw_json(&book).unwrap();
    drop((book, blueprint, entities, entries));

    // PT: 49 mil silos 9×9 passam do teto de tiles: a análise recusa sem estourar a memória.
    // EN: 49k 9×9 silos exceed the tile cap: the analysis refuses without blowing memory.
    let options = AnalysisOptions::default();
    let result = analyze(&data(), &text, 2, &options, &Limits::default());
    assert_eq!(result.unwrap_err().code(), "world.too_many_tiles");

    let stats = dhat::HeapStats::get();
    eprintln!("pico / peak: {} MB", stats.max_bytes / (1024 * 1024));
    assert!(
        stats.max_bytes < MAX_PEAK_BYTES,
        "pico / peak: {} MB",
        stats.max_bytes / (1024 * 1024)
    );
}
