//! PT: Conversão de blueprints do Factorio 1.1 para o formato 2.0: direções em 16 passos,
//!     nomes de entidades renomeados e conexões de fio (`neighbours`/`connections`) virando `wires`.
//!     As conexões antigas são lidas com tetos fixos, e o total de fios é limitado enquanto cresce.
//! EN: Conversion of Factorio 1.1 blueprints to the 2.0 format: 16-step directions, renamed
//!     entities and wire connections (`neighbours`/`connections`) turned into `wires`. Old
//!     connections are read with fixed caps, and the wire total is limited while it grows.

use std::collections::{BTreeSet, HashSet};

use serde::Deserialize;

use crate::bounded::bounded_vec;
use crate::error::DecodeError;
use crate::limits::HARD_MAX_WIRES;
use crate::model::{Blueprint, GAME_VERSION, RawJson, major_version};

/// PT: Conector de cobre de poste (e lado esquerdo do interruptor) no formato do 2.0.
/// EN: Pole copper connector (and power switch left side) in the 2.0 format.
pub const COPPER_CONNECTOR: u64 = 5;
/// PT: Lado direito de cobre do interruptor de energia. EN: Power switch right copper side.
pub const COPPER_RIGHT_CONNECTOR: u64 = 6;

/// PT: Teto de conexões por lista no formato antigo. EN: Cap on connections per list in the old format.
const MAX_LEGACY_LINKS: usize = 64;

/// PT: Nomes do 1.1 que mudaram no 2.0.
/// EN: 1.1 names that changed in 2.0.
const RENAMES: &[(&str, &str)] = &[
    ("filter-inserter", "fast-inserter"),
    ("stack-inserter", "bulk-inserter"),
    ("stack-filter-inserter", "bulk-inserter"),
    ("logistic-chest-active-provider", "active-provider-chest"),
    ("logistic-chest-passive-provider", "passive-provider-chest"),
    ("logistic-chest-storage", "storage-chest"),
    ("logistic-chest-buffer", "buffer-chest"),
    ("logistic-chest-requester", "requester-chest"),
    ("straight-rail", "legacy-straight-rail"),
    ("curved-rail", "legacy-curved-rail"),
];

#[derive(Deserialize)]
struct Neighbours(#[serde(deserialize_with = "bounded_vec::<_, _, MAX_LEGACY_LINKS>")] Vec<u64>);

#[derive(Deserialize)]
struct Target {
    entity_id: u64,
    #[serde(default)]
    circuit_id: Option<u64>,
}

#[derive(Deserialize, Default)]
struct Side {
    #[serde(default, deserialize_with = "bounded_vec::<_, _, MAX_LEGACY_LINKS>")]
    red: Vec<Target>,
    #[serde(default, deserialize_with = "bounded_vec::<_, _, MAX_LEGACY_LINKS>")]
    green: Vec<Target>,
}

#[derive(Deserialize)]
struct CopperTarget {
    entity_id: u64,
}

#[derive(Deserialize, Default)]
struct Connections {
    #[serde(rename = "1", default)]
    one: Option<Side>,
    #[serde(rename = "2", default)]
    two: Option<Side>,
    #[serde(
        rename = "Cu0",
        default,
        deserialize_with = "bounded_vec::<_, _, MAX_LEGACY_LINKS>"
    )]
    copper_left: Vec<CopperTarget>,
    #[serde(
        rename = "Cu1",
        default,
        deserialize_with = "bounded_vec::<_, _, MAX_LEGACY_LINKS>"
    )]
    copper_right: Vec<CopperTarget>,
}

// PT: Erro de leitura das conexões antigas: limite atingido ou JSON inválido.
// EN: Error reading old connections: limit hit or invalid JSON.
fn invalid_legacy(_: serde_json::Error) -> DecodeError {
    match crate::bounded::take_limit_hit() {
        Some(max) => DecodeError::LimitExceeded {
            what: "legacy_links",
            found: max.saturating_add(1),
            max,
        },
        None => DecodeError::InvalidJson {
            category: "legacy",
            line: 0,
            column: 0,
        },
    }
}

// PT: Acrescenta um fio sem passar do teto.
// EN: Adds a wire without exceeding the cap.
fn push_wire(wires: &mut BTreeSet<[u64; 4]>, wire: [u64; 4]) -> Result<(), DecodeError> {
    wires.insert(wire);
    if wires.len() > HARD_MAX_WIRES {
        return Err(DecodeError::LimitExceeded {
            what: "wires",
            found: wires.len(),
            max: HARD_MAX_WIRES,
        });
    }
    Ok(())
}

// PT: Ordena as duas pontas para o mesmo fio não entrar duas vezes.
// EN: Orders both ends so the same wire is not added twice.
fn ordered(a: u64, connector_a: u64, b: u64, connector_b: u64) -> [u64; 4] {
    if (a, connector_a) <= (b, connector_b) {
        [a, connector_a, b, connector_b]
    } else {
        [b, connector_b, a, connector_a]
    }
}

/// PT: Converte a blueprint para o 2.0 se ela for explicitamente de uma versão anterior.
///     Devolve `true` quando houve conversão.
/// EN: Converts the blueprint to 2.0 if it is explicitly from an earlier version.
///     Returns `true` when a conversion happened.
pub fn upgrade(blueprint: &mut Blueprint) -> Result<bool, DecodeError> {
    // PT: Sem versão é tratado como atual: só o 1.1 explícito é convertido.
    // EN: A missing version is treated as current: only explicit 1.1 is converted.
    if blueprint.version == 0 || major_version(blueprint.version) >= 2 {
        return Ok(false);
    }

    let known: HashSet<u64> = blueprint.entities.iter().map(|e| e.entity_number).collect();
    let mut wires: BTreeSet<[u64; 4]> = blueprint.wires.iter().copied().collect();

    for entity in &mut blueprint.entities {
        // PT: No 1.1 as direções vão de 0 a 7; no 2.0 de 0 a 15.
        // EN: In 1.1 directions go from 0 to 7; in 2.0 from 0 to 15.
        entity.direction = entity.direction.saturating_mul(2);

        if let Some((_, new_name)) = RENAMES.iter().find(|(old, _)| *old == entity.name) {
            entity.name = (*new_name).to_owned();
        }

        let number = entity.entity_number;
        if let Some(raw) = entity.extra.remove("neighbours") {
            let Neighbours(neighbours) = raw.parse().map_err(invalid_legacy)?;
            for neighbour in neighbours {
                if known.contains(&neighbour) && neighbour != number {
                    push_wire(
                        &mut wires,
                        ordered(number, COPPER_CONNECTOR, neighbour, COPPER_CONNECTOR),
                    )?;
                }
            }
        }
        if let Some(raw) = entity.extra.remove("connections") {
            let connections: Connections = parse_connections(&raw)?;
            for (side, output) in [(&connections.one, false), (&connections.two, true)] {
                let Some(side) = side else { continue };
                for (targets, red) in [(&side.red, true), (&side.green, false)] {
                    for target in targets {
                        if !known.contains(&target.entity_id) {
                            continue;
                        }
                        let target_output = target.circuit_id == Some(2);
                        let from = circuit_connector(red, output);
                        let to = circuit_connector(red, target_output);
                        push_wire(&mut wires, ordered(number, from, target.entity_id, to))?;
                    }
                }
            }
            for (targets, connector) in [
                (&connections.copper_left, COPPER_CONNECTOR),
                (&connections.copper_right, COPPER_RIGHT_CONNECTOR),
            ] {
                for target in targets {
                    if known.contains(&target.entity_id) && target.entity_id != number {
                        push_wire(
                            &mut wires,
                            ordered(number, connector, target.entity_id, COPPER_CONNECTOR),
                        )?;
                    }
                }
            }
        }
    }

    blueprint.wires = wires.into_iter().collect();
    blueprint.version = GAME_VERSION;
    Ok(true)
}

fn parse_connections(raw: &RawJson) -> Result<Connections, DecodeError> {
    raw.parse().map_err(invalid_legacy)
}

// PT: Conector de circuito do 2.0: vermelho 1/3 e verde 2/4 (entrada/saída dos combinadores).
// EN: 2.0 circuit connector: red 1/3 and green 2/4 (combinator input/output).
fn circuit_connector(red: bool, output: bool) -> u64 {
    match (red, output) {
        (true, false) => 1,
        (false, false) => 2,
        (true, true) => 3,
        (false, true) => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, Position, pack_version};
    use serde_json::json;

    fn raw(value: serde_json::Value) -> RawJson {
        RawJson::from_value(&value).unwrap()
    }

    #[test]
    fn converts_directions_names_and_wires() {
        let mut first = Entity::new(1, "small-electric-pole", Position { x: 0.5, y: 0.5 });
        first.extra.insert("neighbours".into(), raw(json!([2])));
        let mut second = Entity::new(2, "filter-inserter", Position { x: 1.5, y: 0.5 });
        second.direction = 2;
        second
            .extra
            .insert("neighbours".into(), raw(json!([1, 99])));
        let mut switch = Entity::new(3, "power-switch", Position { x: 4.0, y: 4.0 });
        switch.extra.insert(
            "connections".into(),
            raw(json!({"Cu1": [{"entity_id": 1, "wire_id": 0}]})),
        );

        let mut blueprint = Blueprint {
            entities: vec![first, second, switch],
            version: pack_version(1, 1, 110, 0),
            ..Blueprint::default()
        };
        assert!(upgrade(&mut blueprint).unwrap());

        assert_eq!(blueprint.entities[1].direction, 4);
        assert_eq!(blueprint.entities[1].name, "fast-inserter");
        assert_eq!(blueprint.wires, vec![[1, 5, 2, 5], [1, 5, 3, 6]]);
        assert!(!blueprint.entities[0].extra.contains_key("neighbours"));
        assert_eq!(major_version(blueprint.version), 2);
    }

    #[test]
    fn converts_circuit_connections() {
        let mut combinator = Entity::new(1, "arithmetic-combinator", Position { x: 0.5, y: 1.0 });
        combinator.extra.insert(
            "connections".into(),
            raw(json!({"2": {"green": [{"entity_id": 2, "circuit_id": 1}]}})),
        );
        let lamp = Entity::new(2, "small-lamp", Position { x: 0.5, y: 3.5 });
        let mut blueprint = Blueprint {
            entities: vec![combinator, lamp],
            version: pack_version(1, 1, 110, 0),
            ..Blueprint::default()
        };
        upgrade(&mut blueprint).unwrap();
        assert_eq!(blueprint.wires, vec![[1, 4, 2, 2]]);
    }

    #[test]
    fn rejects_oversized_legacy_lists() {
        let many: Vec<u64> = (1..=100).collect();
        let mut pole = Entity::new(1, "small-electric-pole", Position { x: 0.5, y: 0.5 });
        pole.extra.insert("neighbours".into(), raw(json!(many)));
        let mut blueprint = Blueprint {
            entities: vec![pole],
            version: pack_version(1, 1, 110, 0),
            ..Blueprint::default()
        };
        assert!(upgrade(&mut blueprint).is_err());
    }

    #[test]
    fn leaves_2_0_untouched() {
        let mut entity = Entity::new(1, "inserter", Position { x: 0.5, y: 0.5 });
        entity.direction = 4;
        let mut blueprint = Blueprint {
            entities: vec![entity],
            ..Blueprint::default()
        };
        assert!(!upgrade(&mut blueprint).unwrap());
        assert_eq!(blueprint.entities[0].direction, 4);
    }
}
