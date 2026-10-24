//! PT: Cálculo com os dados reais do jogo (pulado sem o pacote, salvo `BF_REQUIRE_GAME_ASSETS=1`).
//! EN: Calculation with the real game data (skipped without the package, unless `BF_REQUIRE_GAME_ASSETS=1`).

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;

use bf_analysis::analyze;
use bf_blueprint::{Blueprint, BlueprintString, Limits, encode};
use bf_calc::AnalysisOptions;
use bf_data::GameData;
use serde_json::json;

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

fn blueprint(entities: serde_json::Value) -> String {
    let content: Blueprint = serde_json::from_value(json!({
        "item": "blueprint",
        "version": 562954249895936u64,
        "entities": entities,
    }))
    .unwrap();
    encode(&BlueprintString::Blueprint(content)).unwrap()
}

fn speed_modules(count: u32, inventory: u32) -> serde_json::Value {
    let positions: Vec<_> = (0..count)
        .map(|stack| json!({"inventory": inventory, "stack": stack}))
        .collect();
    json!([{ "id": { "name": "speed-module" }, "items": { "in_inventory": positions } }])
}

#[test]
fn modules_and_beacon_add_up() {
    let Some(data) = load() else { return };
    let text = blueprint(json!([
        { "entity_number": 1, "name": "assembling-machine-2", "position": { "x": 1.5, "y": 1.5 },
          "recipe": "iron-gear-wheel", "items": speed_modules(2, 4) },
        { "entity_number": 2, "name": "beacon", "position": { "x": 4.5, "y": 1.5 },
          "items": speed_modules(2, 1) }
    ]));
    let analysis = analyze(
        &data,
        &text,
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap();
    let machine = &analysis.report.machines[0];

    // PT: 2 módulos: +0,4 velocidade e +1,0 consumo. Beacon: 2 × (0,2; 0,5) × 1,5 = (0,6; 1,5).
    // EN: 2 modules: +0.4 speed and +1.0 consumption. Beacon: 2 × (0.2; 0.5) × 1.5 = (0.6; 1.5).
    assert_eq!(machine.beacons, 1);
    assert!((machine.effect.speed - 1.0).abs() < 1e-9);
    assert!((machine.effect.consumption - 2.5).abs() < 1e-9);
    // PT: 0,75 × 2 / 0,5 s = 3 engrenagens/s. EN: 0.75 × 2 / 0.5 s = 3 gears/s.
    assert!((machine.crafts_per_second - 3.0).abs() < 1e-9);
    assert!((machine.power_w - 150_000.0 * 3.5).abs() < 1e-6);
    assert!((machine.drain_w - 5_000.0).abs() < 1e-6);

    let gear = analysis
        .report
        .nominal
        .iter()
        .find(|f| f.name == "iron-gear-wheel")
        .unwrap();
    assert!((gear.produced_per_second - 3.0).abs() < 1e-9);
    let plate = analysis
        .report
        .nominal
        .iter()
        .find(|f| f.name == "iron-plate")
        .unwrap();
    assert!((plate.consumed_per_second - 6.0).abs() < 1e-9);
    assert!((analysis.report.power.by_category["beacon"] - 480_000.0).abs() < 1e-6);
}

#[test]
fn red_science_line_is_limited_by_gears() {
    let Some(data) = load() else { return };
    // PT: 1 montadora de engrenagem (1,5/s em AM2) alimenta 5 de ciência vermelha (5 × 0,15 = 0,75 ciclos/s,
    //     que pedem 0,75 engrenagens/s): a engrenagem sobra e para metade do tempo.
    // EN: 1 gear assembler (1.5/s on AM2) feeds 5 red science (5 × 0.15 = 0.75 cycles/s,
    //     needing 0.75 gears/s): gears are left over and idle half the time.
    let mut entities = vec![json!({ "entity_number": 1, "name": "assembling-machine-2",
        "position": { "x": 1.5, "y": 1.5 }, "recipe": "iron-gear-wheel" })];
    for index in 0..5u64 {
        entities.push(json!({ "entity_number": index + 2, "name": "assembling-machine-2",
            "position": { "x": 4.5 + 3.0 * index as f64, "y": 1.5 }, "recipe": "automation-science-pack" }));
    }
    let analysis = analyze(
        &data,
        &blueprint(json!(entities)),
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap();
    let state = &analysis.report.steady_state;
    assert!(state.solved);
    assert!((state.utilization["automation-science-pack"] - 1.0).abs() < 1e-6);
    // PT: A sobra de engrenagem entope a saída: a montadora de engrenagem roda a 50%.
    // EN: Surplus gears clog the output: the gear assembler runs at 50%.
    assert!((state.utilization["iron-gear-wheel"] - 0.5).abs() < 1e-6);

    // PT: Com 15 de ciência a engrenagem não dá conta: 2,25 pedidas, 1,5 produzidas → 66,7%.
    // EN: With 15 science assemblers gears cannot keep up: 2.25 needed, 1.5 produced → 66.7%.
    let mut entities = vec![json!({ "entity_number": 1, "name": "assembling-machine-2",
        "position": { "x": 1.5, "y": 1.5 }, "recipe": "iron-gear-wheel" })];
    for index in 0..15u64 {
        entities.push(json!({ "entity_number": index + 2, "name": "assembling-machine-2",
            "position": { "x": 4.5 + 3.0 * index as f64, "y": 1.5 }, "recipe": "automation-science-pack" }));
    }
    let analysis = analyze(
        &data,
        &blueprint(json!(entities)),
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap();
    let state = &analysis.report.steady_state;
    assert!((state.utilization["automation-science-pack"] - 2.0 / 3.0).abs() < 1e-6);
    assert_eq!(state.bottlenecks, vec!["iron-gear-wheel".to_owned()]);
}

#[test]
fn furnace_without_recipe_warns_and_override_fixes_it() {
    let Some(data) = load() else { return };
    let text = blueprint(json!([
        { "entity_number": 1, "name": "stone-furnace", "position": { "x": 1.0, "y": 1.0 } }
    ]));
    let analysis = analyze(
        &data,
        &text,
        0,
        &AnalysisOptions::default(),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(analysis.report.warnings.len(), 1);

    let mut options = AnalysisOptions::default();
    options.recipe_overrides.insert(1, "iron-plate".into());
    let analysis = analyze(&data, &text, 0, &options, &Limits::default()).unwrap();
    assert!(analysis.report.warnings.is_empty());
    let plate = analysis
        .report
        .nominal
        .iter()
        .find(|f| f.name == "iron-plate")
        .unwrap();
    // PT: Fornalha de pedra: velocidade 1, placa de ferro 3,2 s → 0,3125/s.
    // EN: Stone furnace: speed 1, iron plate 3.2 s → 0.3125/s.
    assert!((plate.produced_per_second - 0.3125).abs() < 1e-9);
    // PT: Consome carvão como combustível: 90 kW / 4 MJ = 0,0225/s.
    // EN: Burns coal as fuel: 90 kW / 4 MJ = 0.0225/s.
    let coal = analysis
        .report
        .nominal
        .iter()
        .find(|f| f.name == "coal")
        .unwrap();
    assert!((coal.consumed_per_second - 0.0225).abs() < 1e-9);
}
