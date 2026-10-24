//! PT: Simulação das blueprints de referência contra o que o jogo mediu.
//! EN: Simulation of the reference blueprints against what the game measured.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::PathBuf;

use bf_blueprint::{BlueprintString, Limits, decode};
use bf_calc::{AnalysisOptions, calculate};
use bf_data::GameData;
use bf_sim::FlowReport;
use bf_world::{Topology, World};

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

pub fn steady(data: &GameData, name: &str) -> FlowReport {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../validation/golden")
        .join(name);
    let text = std::fs::read_to_string(path).expect(name);
    let BlueprintString::Blueprint(blueprint) = decode(&text, &Limits::default()).unwrap().content
    else {
        panic!("{name}");
    };
    let world = World::build(&blueprint, data).unwrap();
    let topology = Topology::build(&world, &blueprint, data);
    let options = AnalysisOptions::default();
    let report = calculate(data, &blueprint, &world, &options).unwrap();
    let input = bf_sim::build(
        data,
        &world,
        &topology,
        &report,
        options.inserter_research,
        "coal",
    );
    bf_sim::steady::simulate(&input).unwrap()
}

// PT: Medido no jogo (cenário golden-ciencia-1-2): 0,75/s de cada ciência, 1,875 engrenagem/s e
//     0,75 circuito/s.
// EN: Measured in-game (golden-ciencia-1-2 scenario): 0.75/s of each science, 1.875 gear/s and
//     0.75 circuit/s.
#[test]
fn science_line_matches_the_game() {
    let Some(data) = load() else { return };
    let flow = steady(&data, "07-ciencia1-2.txt");
    println!(
        "{:?} rounds {} converged {}",
        flow.produced, flow.rounds, flow.converged
    );
    for m in &flow.machines {
        println!(
            "{} {:.4} {:?}",
            m.entity_number, m.crafts_per_second, m.limit
        );
    }
    let close = |item: &str, expected: f64| {
        let got = flow.produced.get(item).copied().unwrap_or(0.0);
        assert!(
            (got - expected).abs() <= expected * 0.01,
            "{item}: {got} vs {expected}"
        );
    };
    close("automation-science-pack", 0.75);
    close("logistic-science-pack", 0.75);
    close("iron-gear-wheel", 1.875);
    close("electronic-circuit", 0.75);
    assert!(flow.converged);
}

// PT: Splitter com a saída esquerda fechada e a direita entrando de lado noutra esteira, que
//     termina numa saída: a simulação por tick precisa levar os itens pela entrada lateral.
// EN: Splitter with the left output closed and the right one sideloading into another belt that
//     ends at an output: the tick simulation must carry the items through the sideload.
#[test]
fn tick_splitter_sideload_exit() {
    let Some(data) = load() else { return };
    let mut entities = vec![
        serde_json::json!({"entity_number": 1, "name": "constant-combinator",
        "position": {"x": -2.5, "y": 0.5}, "control_behavior": {"sections": {"sections": [
        {"index": 1, "filters": [{"index": 1, "name": "iron-plate", "quality": "normal", "comparator": "=", "count": 1}]}]}}}),
    ];
    let mut add = |name: &str, x: f64, y: f64, direction: u8| {
        let n = entities.len() + 1;
        entities.push(serde_json::json!({"entity_number": n, "name": name, "position": {"x": x, "y": y}, "direction": direction}));
    };
    add("transport-belt", -1.5, 0.5, 4);
    add("transport-belt", -0.5, 0.5, 4);
    add("splitter", 0.5, 0.0, 4);
    // PT: À frente da metade direita (y = 0) passa uma esteira para o sul, com outra atrás dela.
    // EN: Ahead of the right half (y = 0) runs a southward belt, with another one behind it.
    add("transport-belt", 1.5, -0.5, 8);
    for y in 0..6 {
        add("transport-belt", 1.5, f64::from(y) + 0.5, 8);
    }
    let text = bf_blueprint::encode_raw_json(
        &serde_json::json!({"blueprint": {"item": "blueprint", "version": bf_blueprint::GAME_VERSION, "entities": entities}})
            .to_string(),
    )
    .unwrap();
    let BlueprintString::Blueprint(blueprint) = decode(&text, &Limits::default()).unwrap().content
    else {
        panic!()
    };
    let world = World::build(&blueprint, &data).unwrap();
    let topology = Topology::build(&world, &blueprint, &data);
    let options = AnalysisOptions::default();
    let report = calculate(&data, &blueprint, &world, &options).unwrap();
    let input = bf_sim::build(
        &data,
        &world,
        &topology,
        &report,
        options.inserter_research,
        "coal",
    );
    let tick = bf_sim::tick::simulate_with(
        &input,
        &bf_sim::tick::TickOptions {
            warmup_ticks: 600,
            measure_ticks: 3600,
            ..Default::default()
        },
        &mut |_| true,
    )
    .unwrap();
    let drained = tick.drained.get("iron-plate").copied().unwrap_or(0.0);
    assert!(drained > 5.0, "{drained}");
}
