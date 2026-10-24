//! PT: Topologia (esteiras, insersores, energia, fluidos) das blueprints de referência do jogo.
//! EN: Topology (belts, inserters, power, fluids) of the game's reference blueprints.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::PathBuf;

use bf_blueprint::{Blueprint, BlueprintString, Limits, decode};
use bf_data::GameData;
use bf_world::belts::{Output, TileKind};
use bf_world::inserters::Endpoint;
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

fn golden(data: &GameData, name: &str) -> (Blueprint, World, Topology) {
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
    (blueprint, world, topology)
}

// PT: 03: 10 pares de subterrâneas e uma sem par, o "T" com duas entradas laterais, curvas em L.
// EN: 03: 10 underground pairs and one without a pair, the "T" with two sideloads, L curves.
#[test]
fn belts_follow_the_game_rules() {
    let Some(data) = load() else { return };
    let (_, _, t) = golden(&data, "03-esteiras.txt");
    let count = |f: &dyn Fn(&bf_world::belts::BeltTile) -> bool| {
        t.belts.tiles.iter().filter(|x| f(x)).count()
    };
    assert_eq!(
        count(&|x| matches!(x.output, Output::Underground { .. })),
        10
    );
    assert_eq!(
        count(&|x| x.output == Output::None && x.kind == TileKind::UndergroundInput),
        1
    );
    assert_eq!(count(&|x| matches!(x.output, Output::Sideload { .. })), 2);
    assert!(count(&|x| x.curve.is_some()) >= 2);
    assert_eq!(t.belts.splitters.len(), 21);
    assert!(
        t.belts
            .splitters
            .iter()
            .any(|s| s.filter.as_deref() == Some("iron-plate"))
    );
}

// PT: 04 e 07: todo insersor tem algo nas duas pontas; na 07 tudo tem energia e nenhuma rede de
//     fluido mistura fluidos.
// EN: 04 and 07: every inserter has something at both ends; in 07 everything is powered and no
//     fluid network mixes fluids.
#[test]
fn inserters_power_and_fluids() {
    let Some(data) = load() else { return };
    for name in ["04-insersores.txt", "07-ciencia1-2.txt"] {
        let (_, _, t) = golden(&data, name);
        assert!(!t.inserters.is_empty(), "{name}");
        assert!(
            t.inserters
                .iter()
                .all(|i| i.pickup != Endpoint::Nothing && i.drop != Endpoint::Nothing),
            "{name}"
        );
    }
    let (_, world, t) = golden(&data, "07-ciencia1-2.txt");
    assert!(t.power.has_poles);
    assert_eq!(t.power.networks, 1);
    assert!((0..world.entities.len()).all(|e| t.power.powered(e)));
    let (_, _, t) = golden(&data, "06-quimica.txt");
    assert!(!t.fluids.networks.is_empty());
    assert_eq!(t.fluids.mixed().count(), 0);
}
