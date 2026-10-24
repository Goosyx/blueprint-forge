//! PT: Entradas hostis para o cálculo (achados do code review da Fase 2): beacons e máquinas
//!     empilhados no mesmo lugar devem ser recusados pelo orçamento, rápido.
//! EN: Hostile inputs for the calculation (Phase 2 code review findings): beacons and machines
//!     stacked in the same place must be refused by the budget, quickly.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use bf_analysis::analyze;
use bf_blueprint::{Limits, encode_raw_json};
use bf_calc::AnalysisOptions;
use bf_data::{Entity, EntityKind, GameData, TileSize};

fn data() -> GameData {
    let mut entities = BTreeMap::new();
    let mut add = |name: &str, kind: EntityKind| {
        entities.insert(
            name.to_owned(),
            Entity {
                name: name.into(),
                prototype: name.into(),
                size: TileSize {
                    width: 3,
                    height: 3,
                },
                collision_box: [[-1.2, -1.2], [1.2, 1.2]],
                placed_by: Vec::new(),
                energy_source: None,
                energy_usage_w: None,
                kind,
            },
        );
    };
    add(
        "assembling-machine-2",
        EntityKind::CraftingMachine {
            crafting_speed: 0.75,
            crafting_categories: vec!["crafting".into()],
            module_slots: 2,
            allowed_effects: vec!["speed".into()],
            uses_module_effects: true,
            uses_beacon_effects: true,
            fixed_recipe: None,
            fluid_boxes: Vec::new(),
        },
    );
    add(
        "beacon",
        EntityKind::Beacon {
            supply_area_distance: 3.0,
            distribution_effectivity: 1.5,
            profile: vec![1.0, 0.7, 0.6, 0.5],
            module_slots: 2,
            allowed_effects: vec!["speed".into()],
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
fn stacked_beacons_hit_the_budget_quickly() {
    let mut entities = Vec::new();
    for number in 1..=20_000u64 {
        let name = if number % 2 == 0 {
            "beacon"
        } else {
            "assembling-machine-2"
        };
        entities.push(format!(
            r#"{{"entity_number":{number},"name":"{name}","position":{{"x":1.5,"y":1.5}}}}"#
        ));
    }
    let json = format!(
        r#"{{"blueprint":{{"item":"blueprint","version":562954249895936,"entities":[{}]}}}}"#,
        entities.join(",")
    );
    let text = encode_raw_json(&json).unwrap();
    let started = Instant::now();
    let error = analyze(
        &data(),
        &text,
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "analysis.budget_exceeded");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "lento / slow: {:?}",
        started.elapsed()
    );
}

#[test]
fn huge_options_are_refused_before_working() {
    let mut options = AnalysisOptions::default();
    for index in 0..1_000 {
        options.exported.insert(format!("item-{index:0>90}"));
    }
    let error = analyze(
        &data(),
        "0eJyrVkrOz0vLTFeyMqgFAB5UBHw=",
        0,
        &options,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "analysis.options_too_large");
}
