//! PT: Ida e volta: o que é gravado volta igual na leitura, inclusive campos não interpretados.
//! EN: Round trip: what is written comes back unchanged when read, including uninterpreted fields.

#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use bf_blueprint::{
    Blueprint, BlueprintBook, BlueprintString, BookEntry, Entity, Icon, ItemRequests, Limits,
    Position, RawJson, Signal, decode, encode,
};
use proptest::prelude::*;
use serde_json::json;

fn roundtrip(content: &BlueprintString) -> BlueprintString {
    let text = encode(content).unwrap();
    decode(&text, &Limits::default()).unwrap().content
}

#[test]
fn keeps_unknown_entity_fields_and_books() {
    let mut chest = Entity::new(1, "requester-chest", Position { x: 0.5, y: 0.5 });
    chest.extra.insert(
        "request_filters".into(),
        RawJson::from_value(
            &json!({"sections":[{"index":1,"filters":[{"index":1,"name":"iron-plate","count":100}]}]}),
        )
        .unwrap(),
    );
    let mut inserter = Entity::new(2, "inserter", Position { x: 0.5, y: 1.5 });
    inserter.direction = 8;

    let blueprint = Blueprint {
        label: Some("Teste ção".into()),
        icons: vec![Icon {
            signal: Signal {
                kind: Some("item".into()),
                name: "iron-plate".into(),
                quality: None,
            },
            index: 1,
        }],
        entities: vec![chest, inserter],
        ..Blueprint::default()
    };
    let book = BlueprintString::Book(BlueprintBook {
        item: "blueprint-book".into(),
        label: Some("Livro".into()),
        label_color: None,
        description: None,
        icons: Vec::new(),
        blueprints: vec![BookEntry {
            index: 0,
            blueprint: Some(blueprint),
            blueprint_book: None,
            upgrade_planner: None,
            deconstruction_planner: None,
        }],
        active_index: 0,
        version: bf_blueprint::GAME_VERSION,
    });

    assert_eq!(roundtrip(&book), book);
}

#[test]
fn reads_modules_in_both_formats() {
    let modern = json!([{"id":{"name":"speed-module"},"items":{"in_inventory":[{"inventory":4,"stack":0},{"inventory":4,"stack":1}]}}]);
    let parsed: ItemRequests = serde_json::from_value(modern).unwrap();
    assert_eq!(parsed.counts()["speed-module"], 2);

    let legacy: ItemRequests = serde_json::from_value(json!({"productivity-module": 3})).unwrap();
    assert_eq!(legacy.counts()["productivity-module"], 3);
}

#[test]
fn reads_a_book_generated_by_the_old_project() {
    let text = include_str!("fixtures/old_project_green_book.txt");
    let decoded = decode(text, &Limits::default()).unwrap();
    let BlueprintString::Book(book) = &decoded.content else {
        panic!("esperava livro / expected a book");
    };
    assert!(!book.blueprints.is_empty());
    assert!(decoded.warnings.is_empty());
    assert_eq!(roundtrip(&decoded.content), decoded.content);
}

#[test]
fn converts_legacy_strings() {
    let json = json!({"blueprint":{"item":"blueprint","version":281479278886912u64,"entities":[
        {"entity_number":1,"name":"stack-inserter","position":{"x":0.5,"y":0.5},"direction":2},
        {"entity_number":2,"name":"small-electric-pole","position":{"x":1.5,"y":0.5},"neighbours":[3]},
        {"entity_number":3,"name":"small-electric-pole","position":{"x":5.5,"y":0.5},"neighbours":[2]}
    ]}});
    let text = encode_raw(&json);
    let decoded = decode(&text, &Limits::default()).unwrap();
    let BlueprintString::Blueprint(blueprint) = decoded.content else {
        panic!("esperava blueprint / expected a blueprint");
    };
    assert_eq!(blueprint.entities[0].name, "bulk-inserter");
    assert_eq!(blueprint.entities[0].direction, 4);
    assert_eq!(blueprint.wires, vec![[2, 5, 3, 5]]);
    assert_eq!(
        decoded.warnings,
        vec![bf_blueprint::Warning::ConvertedFromLegacy]
    );
}

fn encode_raw(json: &serde_json::Value) -> String {
    use base64::Engine;
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(json.to_string().as_bytes()).unwrap();
    format!(
        "0{}",
        base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap())
    )
}

// PT: Gera entidades com nomes, posições em meio tile e direções válidas.
// EN: Generates entities with names, half-tile positions and valid directions.
fn entity_strategy() -> impl Strategy<Value = (String, i32, i32, u8)> {
    (
        prop::sample::select(vec![
            "inserter",
            "transport-belt",
            "assembling-machine-2",
            "small-electric-pole",
        ]),
        -2000i32..2000,
        -2000i32..2000,
        0u8..16,
    )
        .prop_map(|(name, x, y, direction)| (name.to_owned(), x, y, direction))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn any_valid_blueprint_survives_roundtrip(
        entities in prop::collection::vec(entity_strategy(), 0..60),
        label in proptest::option::of("[\\p{L}0-9 ]{0,40}"),
    ) {
        let entities: Vec<Entity> = entities
            .into_iter()
            .enumerate()
            .map(|(index, (name, x, y, direction))| {
                let mut entity = Entity::new(index as u64 + 1, name, Position {
                    x: f64::from(x) / 2.0,
                    y: f64::from(y) / 2.0,
                });
                entity.direction = direction;
                entity
            })
            .collect();
        let wires = if entities.len() >= 2 { vec![[1, 5, 2, 5]] } else { Vec::new() };
        let content = BlueprintString::Blueprint(Blueprint {
            label,
            entities,
            wires,
            ..Blueprint::default()
        });
        prop_assert_eq!(roundtrip(&content), content);
    }
}
