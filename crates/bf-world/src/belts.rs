//! PT: Topologia das esteiras: cada tile de esteira, subterrânea ou metade de splitter vira um nó
//!     com duas faixas (esquerda e direita, olhando no sentido do movimento) e uma saída. As regras
//!     seguem o jogo: esteira que só recebe de um lado vira curva e mantém as faixas; esteira que
//!     já recebe por trás recebe o lado como entrada lateral (as duas faixas de quem chega vão para
//!     a faixa do lado de onde chegam); a subterrânea de entrada liga na primeira de mesmo nome na
//!     linha, até o alcance máximo; esteira de frente para outra não liga.
//! EN: Belt topology: each belt, underground or splitter-half tile becomes a node with two lanes
//!     (left and right, looking along the movement) and one output. The rules follow the game: a
//!     belt that only receives from one side becomes a curve and keeps the lanes; a belt already
//!     fed from behind takes the side as a sideload (both lanes of the incoming belt go to the
//!     lane on the side they arrive from); an underground input links to the first one of the
//!     same name in line, up to the maximum reach; belts facing each other do not link.

use std::collections::HashMap;

use bf_blueprint::Blueprint;
use bf_data::{EntityKind, GameData};
use serde::Serialize;

use crate::World;
use crate::dir::{Dir, step};

/// PT: Faixa da esquerda (olhando no sentido do movimento). EN: Left lane (looking along the movement).
pub const LEFT: usize = 0;
/// PT: Faixa da direita. EN: Right lane.
pub const RIGHT: usize = 1;

/// PT: Itens por tile e por faixa quando a faixa está cheia (espaçamento de 0,25 tile).
/// EN: Items per tile and per lane when the lane is full (0.25-tile spacing).
pub const ITEMS_PER_LANE_TILE: f64 = 4.0;

/// PT: Tipo do tile. EN: Tile kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TileKind {
    Belt,
    UndergroundInput,
    UndergroundOutput,
    /// PT: Metade de um splitter: `half` 0 é a da esquerda. EN: Splitter half: `half` 0 is the left one.
    Splitter {
        splitter: usize,
        half: usize,
    },
}

/// PT: Para onde o tile manda os itens. EN: Where the tile sends its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output {
    /// PT: Ponta aberta ou bloqueada. EN: Open or blocked end.
    None,
    /// PT: Segue para o tile, faixa por faixa (reta, curva ou entrada de splitter).
    /// EN: Continues into the tile, lane by lane (straight, curve or splitter input).
    Straight { tile: usize },
    /// PT: Entrada lateral: as duas faixas vão para `lane` do tile. EN: Sideload: both lanes go to `lane` of the tile.
    Sideload { tile: usize, lane: usize },
    /// PT: Subterrânea de entrada para a saída pareada, faixa por faixa.
    /// EN: Underground input to the paired output, lane by lane.
    Underground { tile: usize },
}

/// PT: Curva: para que lado a esteira vira. EN: Curve: which way the belt turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Turn {
    Left,
    Right,
}

/// PT: Um tile da rede de esteiras. EN: One tile of the belt network.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BeltTile {
    /// PT: Posição em `World::entities`. EN: Position in `World::entities`.
    pub entity: usize,
    pub x: i64,
    pub y: i64,
    pub dir: Dir,
    pub kind: TileKind,
    /// PT: Velocidade em tiles por tick. EN: Speed in tiles per tick.
    pub speed: f64,
    pub output: Output,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curve: Option<Turn>,
}

impl BeltTile {
    /// PT: Vazão máxima de uma faixa, em itens por segundo (4 itens por tile × 60 ticks).
    /// EN: Maximum throughput of one lane, in items per second (4 items per tile × 60 ticks).
    pub fn lane_capacity(&self) -> f64 {
        self.speed * ITEMS_PER_LANE_TILE * 60.0
    }

    fn sends_forward(&self) -> bool {
        !matches!(self.kind, TileKind::UndergroundInput)
    }
}

/// PT: Lado de prioridade do splitter. EN: Splitter priority side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

/// PT: Splitter com as duas metades e a configuração gravada na blueprint.
/// EN: Splitter with both halves and the configuration stored in the blueprint.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Splitter {
    pub entity: usize,
    /// PT: Tiles da metade esquerda e da direita. EN: Tiles of the left and right halves.
    pub halves: [usize; 2],
    /// PT: Lida da blueprint, mas ainda não usada pelas simulações (a disputa entre as duas
    ///     entradas só importa quando as saídas estão cheias).
    /// EN: Read from the blueprint, but not used by the simulations yet (the contest between both
    ///     inputs only matters when the outputs are full).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_priority: Option<Side>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_priority: Option<Side>,
    /// PT: Item do filtro: vai para o lado de `output_priority`; o resto, para o outro.
    /// EN: Filter item: goes to the `output_priority` side; the rest, to the other one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// PT: Rede de esteiras da blueprint. EN: The blueprint's belt network.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BeltNetwork {
    pub tiles: Vec<BeltTile>,
    pub splitters: Vec<Splitter>,
    #[serde(skip)]
    index: HashMap<(i64, i64), usize>,
}

impl BeltNetwork {
    /// PT: Monta a rede a partir do mundo. Entidades sem direção cardeal são ignoradas.
    /// EN: Builds the network from the world. Entities without a cardinal direction are ignored.
    pub fn build(world: &World, blueprint: &Blueprint, data: &GameData) -> Self {
        let mut network = Self::default();
        for (slot, placed) in world.entities.iter().enumerate() {
            let Some(proto) = data.entities.get(&placed.name) else {
                continue;
            };
            let Some(dir) = Dir::from_direction(placed.direction) else {
                continue;
            };
            let source = blueprint.entities.get(placed.index);
            let origin = (placed.footprint.x, placed.footprint.y);
            match &proto.kind {
                EntityKind::TransportBelt { speed } => {
                    network.push(slot, origin, dir, TileKind::Belt, *speed);
                }
                EntityKind::UndergroundBelt { speed, .. } => {
                    let kind = match source.and_then(|e| e.io_type.as_deref()) {
                        Some("output") => TileKind::UndergroundOutput,
                        _ => TileKind::UndergroundInput,
                    };
                    network.push(slot, origin, dir, kind, *speed);
                }
                EntityKind::Splitter { speed } => {
                    let splitter = network.splitters.len();
                    // PT: A metade esquerda é a que fica mais para o lado esquerdo da direção.
                    //     Só entra na rede um splitter com exatamente dois tiles.
                    // EN: The left half is the one further towards the direction's left side.
                    //     Only a splitter with exactly two tiles enters the network.
                    let (lx, ly) = dir.left().vector();
                    let mut tiles: Vec<(i64, i64)> = placed.footprint.tiles().collect();
                    if tiles.len() != 2 {
                        continue;
                    }
                    tiles.sort_by_key(|(x, y)| -(x * lx + y * ly));
                    let mut halves = [usize::MAX; 2];
                    for (half, tile) in tiles.into_iter().enumerate() {
                        let kind = TileKind::Splitter { splitter, half };
                        if let Some(slot_ref) = halves.get_mut(half) {
                            *slot_ref = network.push(slot, tile, dir, kind, *speed);
                        }
                    }
                    let extra = |key: &str| {
                        source.and_then(|e| e.extra.get(key)).and_then(|raw| {
                            serde_json::from_str::<serde_json::Value>(raw.get()).ok()
                        })
                    };
                    let side = |key: &str| match extra(key).as_ref().and_then(|v| v.as_str()) {
                        Some("left") => Some(Side::Left),
                        Some("right") => Some(Side::Right),
                        _ => None,
                    };
                    let filter = extra("filter")
                        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned));
                    if halves.iter().all(|h| *h != usize::MAX) {
                        network.splitters.push(Splitter {
                            entity: slot,
                            halves,
                            input_priority: side("input_priority"),
                            output_priority: side("output_priority"),
                            filter,
                        });
                    }
                }
                _ => {}
            }
        }
        network.pair_undergrounds(world, data);
        network.link();
        network
    }

    /// PT: Tile da rede na posição, se houver. EN: Network tile at the position, if any.
    pub fn at(&self, x: i64, y: i64) -> Option<usize> {
        self.index.get(&(x, y)).copied()
    }

    /// PT: Tiles que recebem itens deste: o da frente, e no splitter os da frente das duas metades.
    /// EN: Tiles that receive items from this one: the one ahead, and for a splitter those ahead of both halves.
    pub fn successors(&self, index: usize) -> Vec<usize> {
        let target = |tile: &BeltTile| match tile.output {
            Output::Straight { tile }
            | Output::Underground { tile }
            | Output::Sideload { tile, .. } => Some(tile),
            Output::None => None,
        };
        let Some(tile) = self.tiles.get(index) else {
            return Vec::new();
        };
        match tile.kind {
            TileKind::Splitter { splitter, .. } => self
                .splitters
                .get(splitter)
                .map(|s| {
                    s.halves
                        .iter()
                        .filter_map(|h| self.tiles.get(*h).and_then(target))
                        .collect()
                })
                .unwrap_or_default(),
            _ => target(tile).into_iter().collect(),
        }
    }

    /// PT: Cabeças: esteiras comuns que não recebem itens de nenhum tile.
    /// EN: Heads: regular belts that receive items from no tile.
    pub fn heads(&self) -> Vec<usize> {
        let mut fed = vec![false; self.tiles.len()];
        for index in 0..self.tiles.len() {
            for target in self.successors(index) {
                if let Some(slot) = fed.get_mut(target) {
                    *slot = true;
                }
            }
        }
        (0..self.tiles.len())
            .filter(|i| {
                !fed.get(*i).copied().unwrap_or(true)
                    && self.tiles.get(*i).is_some_and(|t| t.kind == TileKind::Belt)
            })
            .collect()
    }

    /// PT: Ordem de quem manda para quem recebe (ordem topológica pela saída); tiles em ciclo
    ///     vão no fim, na ordem do índice. Custo linear.
    /// EN: Order from senders to receivers (topological by output); tiles in a cycle go at the
    ///     end, in index order. Linear cost.
    pub fn order(&self) -> Vec<usize> {
        let count = self.tiles.len();
        let next: Vec<Vec<usize>> = (0..count).map(|i| self.successors(i)).collect();
        let mut incoming = vec![0usize; count];
        for targets in &next {
            for &target in targets {
                if let Some(slot) = incoming.get_mut(target) {
                    *slot += 1;
                }
            }
        }
        let mut ready: Vec<usize> = (0..count)
            .filter(|i| incoming.get(*i) == Some(&0))
            .rev()
            .collect();
        let mut order = Vec::with_capacity(count);
        let mut done = vec![false; count];
        while let Some(tile) = ready.pop() {
            order.push(tile);
            if let Some(slot) = done.get_mut(tile) {
                *slot = true;
            }
            for &target in next.get(tile).map_or(&[][..], Vec::as_slice) {
                if let Some(slot) = incoming.get_mut(target) {
                    *slot = slot.saturating_sub(1);
                    if *slot == 0 {
                        ready.push(target);
                    }
                }
            }
        }
        order.extend((0..count).filter(|i| !done.get(*i).copied().unwrap_or(true)));
        order
    }

    fn push(
        &mut self,
        entity: usize,
        (x, y): (i64, i64),
        dir: Dir,
        kind: TileKind,
        speed: f64,
    ) -> usize {
        let index = self.tiles.len();
        self.tiles.push(BeltTile {
            entity,
            x,
            y,
            dir,
            kind,
            speed,
            output: Output::None,
            curve: None,
        });
        self.index.insert((x, y), index);
        index
    }

    // PT: Cada subterrânea de entrada procura, na linha, a primeira subterrânea de mesmo nome
    //     até o alcance; liga se for uma saída no mesmo sentido.
    // EN: Each underground input looks, in line, for the first underground of the same name
    //     within reach; it links if that is an output in the same direction.
    fn pair_undergrounds(&mut self, world: &World, data: &GameData) {
        let mut links = Vec::new();
        for (index, tile) in self.tiles.iter().enumerate() {
            if tile.kind != TileKind::UndergroundInput {
                continue;
            }
            let Some(name) = world.entities.get(tile.entity).map(|e| e.name.as_str()) else {
                continue;
            };
            let reach = match data.entities.get(name).map(|p| &p.kind) {
                Some(EntityKind::UndergroundBelt { max_distance, .. }) => i64::from(*max_distance),
                _ => 0,
            };
            for distance in 1..=reach {
                let (x, y) = step((tile.x, tile.y), tile.dir, distance);
                let Some(other) = self.at(x, y) else { continue };
                let Some(candidate) = self.tiles.get(other) else {
                    continue;
                };
                let same_name = world
                    .entities
                    .get(candidate.entity)
                    .is_some_and(|e| e.name == name);
                let in_line = candidate.dir == tile.dir || candidate.dir == tile.dir.opposite();
                if !same_name
                    || !in_line
                    || !matches!(
                        candidate.kind,
                        TileKind::UndergroundInput | TileKind::UndergroundOutput
                    )
                {
                    continue;
                }
                if candidate.kind == TileKind::UndergroundOutput && candidate.dir == tile.dir {
                    links.push((index, other));
                }
                break;
            }
        }
        for (input, output) in links {
            if let Some(tile) = self.tiles.get_mut(input) {
                tile.output = Output::Underground { tile: output };
            }
        }
    }

    // PT: Liga cada tile ao que está à sua frente, decidindo reta, curva ou entrada lateral.
    // EN: Links each tile to the one ahead of it, deciding straight, curve or sideload.
    fn link(&mut self) {
        // PT: Quem aponta para cada tile, e de que direção. EN: Who points into each tile, and from which direction.
        let mut feeders: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, tile) in self.tiles.iter().enumerate() {
            if !tile.sends_forward() {
                continue;
            }
            let (x, y) = step((tile.x, tile.y), tile.dir, 1);
            if let Some(target) = self.at(x, y) {
                feeders.entry(target).or_default().push(index);
            }
        }

        let mut outputs = Vec::new();
        let mut curves = Vec::new();
        for (index, tile) in self.tiles.iter().enumerate() {
            if !tile.sends_forward() {
                continue;
            }
            let (x, y) = step((tile.x, tile.y), tile.dir, 1);
            let Some(target) = self.at(x, y) else {
                continue;
            };
            let Some(next) = self.tiles.get(target) else {
                continue;
            };
            if next.dir == tile.dir.opposite() {
                continue;
            }
            let output = if next.dir == tile.dir {
                match next.kind {
                    // PT: A traseira da subterrânea de saída é fechada. EN: The underground output's back is closed.
                    TileKind::UndergroundOutput => Output::None,
                    _ => Output::Straight { tile: target },
                }
            } else {
                let from_behind = feeders.get(&target).is_some_and(|list| {
                    list.iter()
                        .any(|f| self.tiles.get(*f).is_some_and(|t| t.dir == next.dir))
                });
                // PT: A esteira da frente, virada de volta, não alimenta (frente a frente).
                // EN: The belt ahead, turned back, does not feed (head-on).
                let side_count = feeders.get(&target).map_or(0, |list| {
                    list.iter()
                        .filter(|f| {
                            self.tiles
                                .get(**f)
                                .is_some_and(|t| t.dir != next.dir && t.dir != next.dir.opposite())
                        })
                        .count()
                });
                match next.kind {
                    TileKind::Splitter { .. } => Output::None,
                    TileKind::Belt if !from_behind && side_count == 1 => {
                        let turn = if next.dir == tile.dir.right() {
                            Turn::Right
                        } else {
                            Turn::Left
                        };
                        curves.push((target, turn));
                        Output::Straight { tile: target }
                    }
                    _ => {
                        let lane = if tile.dir.opposite() == next.dir.left() {
                            LEFT
                        } else {
                            RIGHT
                        };
                        Output::Sideload { tile: target, lane }
                    }
                }
            };
            outputs.push((index, output));
        }
        for (index, output) in outputs {
            if let Some(tile) = self.tiles.get_mut(index) {
                tile.output = output;
            }
        }
        for (index, turn) in curves {
            if let Some(tile) = self.tiles.get_mut(index) {
                tile.curve = Some(turn);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bf_blueprint::{Entity, Position, RawJson};
    use bf_data::{Entity as Proto, TileSize};
    use std::collections::BTreeMap;

    fn data() -> GameData {
        let mut entities = BTreeMap::new();
        let mut add = |name: &str, prototype: &str, width: u32, kind: EntityKind| {
            entities.insert(
                name.to_owned(),
                Proto {
                    name: name.to_owned(),
                    prototype: prototype.to_owned(),
                    size: TileSize { width, height: 1 },
                    collision_box: [[0.0, 0.0], [0.0, 0.0]],
                    placed_by: Vec::new(),
                    energy_source: None,
                    energy_usage_w: None,
                    fluid_boxes: Vec::new(),
                    kind,
                },
            );
        };
        add(
            "transport-belt",
            "transport-belt",
            1,
            EntityKind::TransportBelt { speed: 0.03125 },
        );
        add(
            "underground-belt",
            "underground-belt",
            1,
            EntityKind::UndergroundBelt {
                speed: 0.03125,
                max_distance: 5,
            },
        );
        add(
            "splitter",
            "splitter",
            2,
            EntityKind::Splitter { speed: 0.03125 },
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

    fn belt(number: u64, name: &str, x: f64, y: f64, direction: u8) -> Entity {
        let mut entity = Entity::new(number, name, Position { x, y });
        entity.direction = direction;
        entity
    }

    fn network(entities: Vec<Entity>) -> BeltNetwork {
        let blueprint = Blueprint {
            entities,
            ..Blueprint::default()
        };
        let data = data();
        let world = World::build(&blueprint, &data).unwrap();
        BeltNetwork::build(&world, &blueprint, &data)
    }

    fn output_at(network: &BeltNetwork, x: i64, y: i64) -> Output {
        network.tiles[network.at(x, y).unwrap()].output
    }

    #[test]
    fn straight_curve_and_head_on() {
        // PT: Leste, leste e depois sul: o último vira curva para a direita.
        // EN: East, east and then south: the last one becomes a right curve.
        let n = network(vec![
            belt(1, "transport-belt", 0.5, 0.5, 4),
            belt(2, "transport-belt", 1.5, 0.5, 4),
            belt(3, "transport-belt", 2.5, 0.5, 8),
            belt(4, "transport-belt", 10.5, 0.5, 4),
            belt(5, "transport-belt", 11.5, 0.5, 12),
        ]);
        let second = n.at(1, 0).unwrap();
        let third = n.at(2, 0).unwrap();
        assert_eq!(output_at(&n, 0, 0), Output::Straight { tile: second });
        assert_eq!(output_at(&n, 1, 0), Output::Straight { tile: third });
        assert_eq!(n.tiles[third].curve, Some(Turn::Right));
        assert_eq!(
            output_at(&n, 10, 0),
            Output::None,
            "frente a frente / head-on"
        );
    }

    #[test]
    fn belt_ahead_facing_back_does_not_undo_the_curve() {
        // PT: A vem do norte para B (leste); C, à frente de B, aponta de volta (oeste).
        // EN: A comes from the north into B (east); C, ahead of B, points back (west).
        let n = network(vec![
            belt(1, "transport-belt", 0.5, -0.5, 8),
            belt(2, "transport-belt", 0.5, 0.5, 4),
            belt(3, "transport-belt", 1.5, 0.5, 12),
        ]);
        let b = n.at(0, 0).unwrap();
        assert_eq!(output_at(&n, 0, -1), Output::Straight { tile: b });
        assert_eq!(n.tiles[b].curve, Some(Turn::Left));
    }

    #[test]
    fn side_input_with_a_belt_behind_is_a_sideload() {
        // PT: Coluna para o sul em x=1 e uma esteira vindo do oeste em (0, 1).
        // EN: Southward column at x=1 and a belt coming from the west at (0, 1).
        let n = network(vec![
            belt(1, "transport-belt", 1.5, 0.5, 8),
            belt(2, "transport-belt", 1.5, 1.5, 8),
            belt(3, "transport-belt", 0.5, 1.5, 4),
        ]);
        let target = n.at(1, 1).unwrap();
        // PT: Indo para o sul, o oeste fica à direita. EN: Going south, west is on the right.
        assert_eq!(
            output_at(&n, 0, 1),
            Output::Sideload {
                tile: target,
                lane: RIGHT
            }
        );
        assert_eq!(n.tiles[target].curve, None);
    }

    #[test]
    fn undergrounds_pair_within_reach_only() {
        let mut input = belt(1, "underground-belt", 0.5, 0.5, 4);
        input.io_type = Some("input".into());
        let mut output = belt(2, "underground-belt", 5.5, 0.5, 4);
        output.io_type = Some("output".into());
        let mut far_input = belt(3, "underground-belt", 0.5, 3.5, 4);
        far_input.io_type = Some("input".into());
        let mut far_output = belt(4, "underground-belt", 6.5, 3.5, 4);
        far_output.io_type = Some("output".into());
        let n = network(vec![input, output, far_input, far_output]);
        let paired = n.at(5, 0).unwrap();
        assert_eq!(output_at(&n, 0, 0), Output::Underground { tile: paired });
        assert_eq!(
            output_at(&n, 0, 3),
            Output::None,
            "6 tiles: fora do alcance / out of reach"
        );
    }

    #[test]
    fn splitter_halves_and_settings() {
        let mut splitter = belt(1, "splitter", 1.0, 0.5, 0);
        splitter.extra.insert(
            "output_priority".into(),
            RawJson::from_value(&"left").unwrap(),
        );
        splitter.extra.insert(
            "filter".into(),
            RawJson::from_value(&serde_json::json!({"name": "iron-plate"})).unwrap(),
        );
        let n = network(vec![splitter, belt(2, "transport-belt", 0.5, -0.5, 0)]);
        let s = &n.splitters[0];
        assert_eq!(
            n.tiles[s.halves[0]].x, 0,
            "esquerda é oeste indo para o norte / left is west going north"
        );
        assert_eq!(n.tiles[s.halves[1]].x, 1);
        assert_eq!(s.output_priority, Some(Side::Left));
        assert_eq!(s.filter.as_deref(), Some("iron-plate"));
        let ahead = n.at(0, -1).unwrap();
        assert_eq!(
            n.tiles[s.halves[0]].output,
            Output::Straight { tile: ahead }
        );
        assert_eq!(n.tiles[s.halves[1]].output, Output::None);
    }
}
