//! PT: Custo da topologia, da dedução de fornalhas e da simulação com blueprints feitas para
//!     explodir o tempo (achados do code review da Fase 3). Cada caso precisa terminar bem abaixo
//!     do teto, mesmo no build de depuração; antes das correções eles levavam de segundos a
//!     minutos no build otimizado.
//! EN: Cost of the topology, the furnace inference and the simulation with blueprints crafted
//!     to blow up the time (Phase 3 code review findings). Each case must finish well below the
//!     cap, even in the debug build; before the fixes they took seconds to minutes in the
//!     optimized build.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bf_analysis::analyze;
use bf_blueprint::{Limits, encode_raw_json};
use bf_calc::AnalysisOptions;
use bf_data::GameData;
use serde_json::{Value, json};

/// PT: Teto por caso, folgado para o build de depuração. EN: Per-case cap, generous for the debug build.
const CAP: Duration = Duration::from_secs(20);

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
        assert!(std::env::var("BF_REQUIRE_GAME_ASSETS").as_deref() != Ok("1"));
        return None;
    }
    Some(GameData::from_json(&std::fs::read_to_string(path).unwrap()).unwrap())
}

struct Builder(Vec<Value>);

impl Builder {
    fn add(&mut self, name: &str, x: f64, y: f64, extra: Value) {
        let mut e =
            json!({"entity_number": self.0.len() + 1, "name": name, "position": {"x": x, "y": y}});
        if let (Some(t), Value::Object(f)) = (e.as_object_mut(), extra) {
            t.extend(f);
        }
        self.0.push(e);
    }

    fn timed(self, data: &GameData, label: &str) {
        let text = encode_raw_json(
            &json!({"blueprint": {"item": "blueprint", "version": bf_blueprint::GAME_VERSION, "entities": self.0}})
                .to_string(),
        )
        .unwrap();
        let started = Instant::now();
        let result = analyze(
            data,
            &text,
            0,
            &AnalysisOptions::default(),
            &Limits::default(),
        );
        let elapsed = started.elapsed();
        eprintln!("{label}: {elapsed:?}");
        assert!(result.is_ok(), "{label}: {:?}", result.err());
        assert!(elapsed < CAP, "{label}: {elapsed:?}");
    }
}

fn marker() -> Value {
    json!({"control_behavior": {"sections": {"sections": [
        {"index": 1, "filters": [{"index": 1, "name": "iron-ore", "quality": "normal", "comparator": "=", "count": 1}]}
    ]}}})
}

// PT: Combinadores e esteiras empilhados no mesmo tile. EN: Combinators and belts stacked on one tile.
#[test]
fn stacked_markers() {
    let Some(data) = load() else { return };
    let mut b = Builder(Vec::new());
    for _ in 0..3000 {
        b.add("constant-combinator", 0.5, 0.5, marker());
        b.add("transport-belt", 1.5, 0.5, json!({"direction": 4}));
    }
    b.timed(&data, "marcadores empilhados / stacked markers");
}

// PT: Esteira longa com insersor em cada tile e fornalhas que não recebem nada.
// EN: Long belt with an inserter on every tile and furnaces that receive nothing.
#[test]
fn furnace_line() {
    let Some(data) = load() else { return };
    let mut b = Builder(Vec::new());
    for x in 0..4000 {
        b.add(
            "transport-belt",
            f64::from(x) + 0.5,
            0.5,
            json!({"direction": 4}),
        );
        b.add("inserter", f64::from(x) + 0.5, 1.5, json!({"direction": 8}));
        if x % 2 == 0 {
            b.add("stone-furnace", f64::from(x) + 1.0, 3.0, json!({}));
        }
    }
    b.timed(&data, "linha de fornalhas / furnace line");
}

// PT: Linha principal com entrada lateral dos dois lados em todo tile. EN: Main line with sideloads on both sides of every tile.
#[test]
fn sideload_comb() {
    let Some(data) = load() else { return };
    let mut b = Builder(Vec::new());
    for x in 0..8000 {
        let x = f64::from(x) + 0.5;
        b.add("transport-belt", x, 0.5, json!({"direction": 4}));
        b.add("transport-belt", x, -0.5, json!({"direction": 8}));
        b.add("transport-belt", x, 1.5, json!({"direction": 0}));
    }
    b.timed(&data, "pente de entradas laterais / sideload comb");
}

// PT: Escada de splitters, cada degrau com uma ponta solta. EN: Splitter staircase, each step with a loose end.
#[test]
fn splitter_staircase() {
    let Some(data) = load() else { return };
    let mut b = Builder(Vec::new());
    for step in 0..3000 {
        let x = f64::from(step) * 2.0;
        b.add(
            "splitter",
            x + 0.5,
            -f64::from(step) + 1.0,
            json!({"direction": 4}),
        );
        b.add(
            "transport-belt",
            x + 1.5,
            -f64::from(step) + 1.5,
            json!({"direction": 4}),
        );
    }
    b.timed(&data, "escada de splitters / splitter staircase");
}

// PT: Postes empilhados e insersores fora do alcance no mesmo bloco. EN: Stacked poles and out-of-reach inserters in the same block.
#[test]
fn stacked_poles() {
    let Some(data) = load() else { return };
    let mut b = Builder(Vec::new());
    for i in 0..20000 {
        b.add("medium-electric-pole", 0.5, 0.5, json!({}));
        b.add("inserter", 12.5, 0.5 + f64::from(i % 3), json!({}));
    }
    b.timed(&data, "postes empilhados / stacked poles");
}

// PT: Muitas esteiras e muitas receitas diferentes (muitas células na simulação estável).
// EN: Many belts and many different recipes (many cells in the steady simulation).
#[test]
fn many_cells() {
    let Some(data) = load() else { return };
    let recipes: Vec<String> = data
        .recipes
        .values()
        .filter(|r| r.categories.iter().any(|c| c == "crafting") && !r.hidden)
        .map(|r| r.name.clone())
        .take(120)
        .collect();
    let mut b = Builder(Vec::new());
    for (k, recipe) in recipes.iter().enumerate() {
        b.add(
            "assembling-machine-2",
            3.0 * k as f64 + 1.5,
            -2.5,
            json!({"recipe": recipe}),
        );
        b.add(
            "inserter",
            3.0 * k as f64 + 1.5,
            -0.5,
            json!({"direction": 0}),
        );
    }
    for x in 0..60000 {
        b.add(
            "transport-belt",
            f64::from(x % 400) + 0.5,
            f64::from(x / 400) + 0.5,
            json!({"direction": 4}),
        );
    }
    b.timed(&data, "muitas células / many cells");
}
