//! PT: Pico de memória das duas simulações numa blueprint sintética de ~10.000 entidades (95
//!     linhas de 10 montadoras com esteiras de entrada e saída). O alocador do `dhat` mede o
//!     maior uso de heap do processo; o valor vai para `validation/measurements/sim-benchmarks.md`.
//! EN: Peak memory of both simulations on a synthetic blueprint of ~10,000 entities (95 rows of
//!     10 assemblers with input and output belts). The `dhat` allocator measures the process's
//!     largest heap use; the value goes into `validation/measurements/sim-benchmarks.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::PathBuf;

use bf_blueprint::{BlueprintString, Limits, decode, encode_raw_json};
use bf_calc::{AnalysisOptions, calculate};
use bf_data::GameData;
use bf_world::{Topology, World};
use serde_json::{Value, json};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

const MAX_PEAK_BYTES: usize = 256 * 1024 * 1024;

fn blueprint(rows: usize) -> String {
    let mut entities = Vec::new();
    let mut add = |name: &str, x: f64, y: f64, extra: Value| {
        let mut e = json!({"entity_number": entities.len() + 1, "name": name, "position": {"x": x, "y": y}});
        if let (Some(t), Value::Object(f)) = (e.as_object_mut(), extra) {
            t.extend(f);
        }
        entities.push(e);
    };
    for row in 0..rows {
        let y = (row * 9) as f64;
        add(
            "constant-combinator",
            -2.5,
            y + 0.5,
            json!({"control_behavior": {"sections": {"sections": [
                {"index": 1, "filters": [{"index": 1, "name": "iron-plate", "quality": "normal", "comparator": "=", "count": 1}]}
            ]}}}),
        );
        for x in -1..32 {
            add(
                "fast-transport-belt",
                f64::from(x) + 0.5,
                y + 0.5,
                json!({"direction": 4}),
            );
            add(
                "fast-transport-belt",
                f64::from(x) + 0.5,
                y + 6.5,
                json!({"direction": 4}),
            );
        }
        for k in 0..10 {
            let x = f64::from(1 + 3 * k);
            add("fast-inserter", x + 0.5, y + 1.5, json!({}));
            add(
                "assembling-machine-2",
                x + 0.5,
                y + 3.5,
                json!({"recipe": "iron-gear-wheel"}),
            );
            add("fast-inserter", x + 0.5, y + 5.5, json!({}));
        }
    }
    encode_raw_json(
        &json!({"blueprint": {"item": "blueprint", "version": bf_blueprint::GAME_VERSION, "entities": entities}})
            .to_string(),
    )
    .unwrap()
}

#[test]
fn simulations_stay_within_memory() {
    let dir = std::env::var_os("BF_GAME_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../game-assets")
                .join(bf_data::GAME_VERSION)
        });
    let Ok(text) = std::fs::read_to_string(dir.join("gamedata.json")) else {
        assert!(std::env::var("BF_REQUIRE_GAME_ASSETS").as_deref() != Ok("1"));
        return;
    };
    let data = GameData::from_json(&text).unwrap();
    let text = blueprint(95);
    let _profiler = dhat::Profiler::builder().testing().build();
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
    let steady = bf_sim::steady::simulate(&input).unwrap();
    let tick = bf_sim::tick::simulate_with(
        &input,
        &bf_sim::tick::TickOptions {
            warmup_ticks: 600,
            measure_ticks: 3600,
            ..bf_sim::tick::TickOptions::default()
        },
        &mut |_| true,
    )
    .unwrap();
    assert!(steady.converged);
    assert!(!tick.machines.is_empty());
    let stats = dhat::HeapStats::get();
    eprintln!(
        "{} entidades / entities: pico / peak {:.1} MB",
        blueprint.entities.len(),
        stats.max_bytes as f64 / (1024.0 * 1024.0)
    );
    assert!(
        stats.max_bytes < MAX_PEAK_BYTES,
        "{} bytes",
        stats.max_bytes
    );
}
