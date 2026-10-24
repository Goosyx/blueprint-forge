//! PT: Dedução das fornalhas com os dados reais do jogo. EN: Furnace inference with the real game data.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;

use bf_blueprint::{BlueprintString, Limits, decode, encode_raw_json};
use bf_calc::{Confidence, infer::infer};
use bf_data::GameData;
use bf_world::{Topology, World};
use serde_json::{Value, json};

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

fn run(data: &GameData, entities: Value) -> Vec<bf_calc::Inference> {
    let text = encode_raw_json(
        &json!({"blueprint": {"item": "blueprint", "version": bf_blueprint::GAME_VERSION, "entities": entities}})
            .to_string(),
    )
    .unwrap();
    let BlueprintString::Blueprint(blueprint) = decode(&text, &Limits::default()).unwrap().content
    else {
        panic!()
    };
    let world = World::build(&blueprint, data).unwrap();
    let topology = Topology::build(&world, &blueprint, data);
    infer(data, &blueprint, &world, &topology)
}

fn entity(number: u64, name: &str, x: f64, y: f64, extra: Value) -> Value {
    let mut e = json!({"entity_number": number, "name": name, "position": {"x": x, "y": y}});
    if let (Some(target), Value::Object(fields)) = (e.as_object_mut(), extra) {
        target.extend(fields);
    }
    e
}

fn marker(number: u64, x: f64, y: f64, item: &str) -> Value {
    entity(
        number,
        "constant-combinator",
        x,
        y,
        json!({"control_behavior": {"sections": {"sections": [
            {"index": 1, "filters": [{"index": 1, "name": item, "quality": "normal", "comparator": "=", "count": 1}]}
        ]}}}),
    )
}

// PT: Minério marcado → placa de ferro; a placa dessa fornalha numa esteira → aço na próxima.
// EN: Marked ore → iron plate; that furnace's plate on a belt → steel in the next one.
#[test]
fn ore_to_plate_to_steel() {
    let Some(data) = load() else { return };
    let mut entities = vec![marker(1, -1.5, 0.5, "iron-ore")];
    for x in 0..4 {
        entities.push(entity(
            2 + x,
            "transport-belt",
            x as f64 + 0.5,
            0.5,
            json!({"direction": 4}),
        ));
    }
    entities.push(entity(10, "inserter", 1.5, 1.5, json!({})));
    entities.push(entity(11, "stone-furnace", 2.0, 3.0, json!({})));
    entities.push(entity(12, "inserter", 1.5, 4.5, json!({})));
    for x in 0..6 {
        entities.push(entity(
            20 + x,
            "transport-belt",
            x as f64 + 0.5,
            5.5,
            json!({"direction": 4}),
        ));
    }
    entities.push(entity(30, "inserter", 4.5, 6.5, json!({})));
    entities.push(entity(31, "stone-furnace", 5.0, 8.0, json!({})));
    let found = run(&data, Value::Array(entities));
    let by = |n: u64| found.iter().find(|i| i.entity_number == n).unwrap();
    assert_eq!(by(11).recipe, "iron-plate");
    assert_eq!(by(11).confidence, Confidence::High);
    assert_eq!(by(31).recipe, "steel-plate");
}

#[test]
fn nothing_arriving_means_no_guess() {
    let Some(data) = load() else { return };
    let found = run(
        &data,
        json!([{"entity_number": 1, "name": "electric-furnace", "position": {"x": 1.5, "y": 1.5}}]),
    );
    assert!(found.is_empty());
}

// PT: Coluna de fornalhas do cenário de fluxo: minério e carvão marcados → todas em placa de ferro.
// EN: Flow scenario furnace column: marked ore and coal → all of them on iron plate.
#[test]
fn furnace_column_scenario() {
    let Some(data) = load() else { return };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../validation/scenarios/flow/flow-furnace-column.json");
    let scenario: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let entities = scenario
        .pointer("/blueprint_json/blueprint/entities")
        .cloned()
        .unwrap();
    let furnaces = entities
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["name"] == "stone-furnace")
        .count();
    let found = run(&data, entities);
    assert_eq!(found.len(), furnaces);
    assert!(
        found
            .iter()
            .all(|i| i.recipe == "iron-plate" && i.confidence == Confidence::High)
    );
}
