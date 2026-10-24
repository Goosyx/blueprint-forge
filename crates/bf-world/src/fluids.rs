//! PT: Redes de fluido. Cada caixa de fluido é um nó; duas conexões de cano se ligam quando uma
//!     aponta para o tile da outra e as duas estão de frente. As posições do protótipo são do
//!     norte, espelhadas na horizontal quando a entidade está espelhada (`mirror`) e giradas pela
//!     direção. O cano subterrâneo procura o par de mesmo nome até o alcance. Uma rede com dois
//!     fluidos diferentes gera aviso, como no jogo.
//! EN: Fluid networks. Each fluid box is a node; two pipe connections link when one points to
//!     the other's tile and both face each other. Prototype positions are north-facing, mirrored
//!     horizontally when the entity is mirrored (`mirror`) and rotated by the direction. The
//!     underground pipe looks for its same-name pair within reach. A network with two different
//!     fluids raises a warning, as in the game.

use std::collections::{BTreeSet, HashMap};

use bf_blueprint::Blueprint;
use bf_data::{EntityKind, GameData, ResourceKind};
use serde::Serialize;

use crate::World;
use crate::dir::{Dir, step, tile_of};

/// PT: Maior alcance de cano subterrâneo aceito, em tiles. EN: Largest accepted underground pipe reach, in tiles.
const MAX_UNDERGROUND: u32 = 64;
use crate::union::{find, union};

/// PT: Caixa de fluido de uma entidade. EN: A fluid box of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct BoxRef {
    pub entity: usize,
    pub index: usize,
}

/// PT: Uma rede de fluido. EN: One fluid network.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FluidNetwork {
    pub boxes: Vec<BoxRef>,
    /// PT: Fluidos que as entidades da rede esperam. EN: Fluids the network's entities expect.
    pub fluids: BTreeSet<String>,
}

/// PT: Ponta subterrânea de um cano, à espera do par. EN: Underground end of a pipe, waiting for its pair.
struct UndergroundEnd<'a> {
    node: usize,
    tile: (i64, i64),
    facing: Dir,
    reach: u32,
    name: &'a str,
}

/// PT: Todas as redes da blueprint. EN: All the blueprint's networks.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FluidNetworks {
    pub networks: Vec<FluidNetwork>,
}

impl FluidNetworks {
    /// PT: Redes com mais de um fluido. EN: Networks with more than one fluid.
    pub fn mixed(&self) -> impl Iterator<Item = &FluidNetwork> {
        self.networks.iter().filter(|n| n.fluids.len() > 1)
    }

    pub fn build(world: &World, blueprint: &Blueprint, data: &GameData) -> Self {
        let mut nodes: Vec<BoxRef> = Vec::new();
        let mut node_of: HashMap<BoxRef, usize> = HashMap::new();
        // PT: (tile, direção para onde aponta) → nó. EN: (tile, direction it points to) → node.
        let mut normal: HashMap<((i64, i64), Dir), usize> = HashMap::new();
        let mut underground: Vec<UndergroundEnd> = Vec::new();

        for (slot, placed) in world.entities.iter().enumerate() {
            let (Some(proto), Some(source)) = (
                data.entities.get(&placed.name),
                blueprint.entities.get(placed.index),
            ) else {
                continue;
            };
            let Some(dir) = Dir::from_direction(placed.direction) else {
                continue;
            };
            let mirrored = source.mirror == Some(true);
            for (index, fluid_box) in proto.fluid_boxes.iter().enumerate() {
                let node = nodes.len();
                let reference = BoxRef {
                    entity: slot,
                    index,
                };
                nodes.push(reference);
                node_of.insert(reference, node);
                for connection in &fluid_box.pipe_connections {
                    let (Some(position), Some(direction)) =
                        (connection.position, connection.direction)
                    else {
                        continue;
                    };
                    let Some(mut facing) = Dir::from_direction(direction) else {
                        continue;
                    };
                    let mut offset = (position[0], position[1]);
                    if mirrored {
                        offset.0 = -offset.0;
                        if matches!(facing, Dir::East | Dir::West) {
                            facing = facing.opposite();
                        }
                    }
                    let (dx, dy) = dir.rotate(offset);
                    let facing = rotate_dir(facing, dir);
                    let tile = tile_of(source.position.x + dx, source.position.y + dy);
                    if connection.connection_type == "underground" {
                        let reach = connection.max_underground_distance.unwrap_or(0);
                        underground.push(UndergroundEnd {
                            node,
                            tile,
                            facing,
                            reach,
                            name: placed.name.as_str(),
                        });
                    } else {
                        normal.insert((tile, facing), node);
                    }
                }
            }
        }

        let mut parent: Vec<usize> = (0..nodes.len()).collect();
        for (&(tile, facing), &node) in &normal {
            let neighbour = step(tile, facing, 1);
            if let Some(&other) = normal.get(&(neighbour, facing.opposite())) {
                union(&mut parent, node, other);
            }
        }
        let by_tile: HashMap<((i64, i64), Dir), &UndergroundEnd> = underground
            .iter()
            .map(|end| ((end.tile, end.facing), end))
            .collect();
        for end in &underground {
            for distance in 1..=i64::from(end.reach.min(MAX_UNDERGROUND)) {
                let target = step(end.tile, end.facing, distance);
                if let Some(other) = by_tile.get(&(target, end.facing.opposite()))
                    && other.name == end.name
                {
                    union(&mut parent, end.node, other.node);
                    break;
                }
            }
        }

        let mut groups: HashMap<usize, Vec<BoxRef>> = HashMap::new();
        for (node, reference) in nodes.iter().enumerate() {
            groups
                .entry(find(&mut parent, node))
                .or_default()
                .push(*reference);
        }
        let mut networks: Vec<FluidNetwork> = groups
            .into_values()
            .map(|mut boxes| {
                boxes.sort();
                let fluids = boxes
                    .iter()
                    .filter_map(|b| expected_fluid(world, blueprint, data, *b))
                    .collect();
                FluidNetwork { boxes, fluids }
            })
            .collect();
        networks.sort_by(|a, b| a.boxes.cmp(&b.boxes));
        Self { networks }
    }
}

// PT: Fluido que a caixa espera: o filtro do protótipo, ou o fluido da receita da máquina na
//     ordem das caixas de entrada e de saída.
// EN: Fluid the box expects: the prototype filter, or the machine recipe's fluid in the order
//     of the input and output boxes.
fn expected_fluid(
    world: &World,
    blueprint: &Blueprint,
    data: &GameData,
    reference: BoxRef,
) -> Option<String> {
    let placed = world.entities.get(reference.entity)?;
    let proto = data.entities.get(&placed.name)?;
    let fluid_box = proto.fluid_boxes.get(reference.index)?;
    if let Some(filter) = &fluid_box.filter {
        return Some(filter.clone());
    }
    if !matches!(proto.kind, EntityKind::CraftingMachine { .. }) {
        return None;
    }
    let recipe = data
        .recipes
        .get(blueprint.entities.get(placed.index)?.recipe.as_deref()?)?;
    let is_input = fluid_box.production_type == "input";
    let order = proto
        .fluid_boxes
        .iter()
        .take(reference.index)
        .filter(|b| b.production_type == fluid_box.production_type)
        .count();
    if is_input {
        recipe
            .ingredients
            .iter()
            .filter(|i| i.kind == ResourceKind::Fluid)
            .nth(order)
            .map(|i| i.name.clone())
    } else if fluid_box.production_type == "output" {
        recipe
            .results
            .iter()
            .filter(|p| p.kind == ResourceKind::Fluid)
            .nth(order)
            .map(|p| p.name.clone())
    } else {
        None
    }
}

fn rotate_dir(facing: Dir, by: Dir) -> Dir {
    match by {
        Dir::North => facing,
        Dir::East => facing.right(),
        Dir::South => facing.opposite(),
        Dir::West => facing.left(),
    }
}
