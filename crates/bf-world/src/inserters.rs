//! PT: De onde cada insersor pega e onde entrega. As posições do protótipo são definidas com o
//!     insersor virado para o norte e giradas pela direção da blueprint, que aponta para o lado
//!     de onde ele pega. Na esteira, a faixa de entrega é a do lado do ponto de entrega: como o
//!     ponto fica 0,2 tile além do centro, o insersor solta na faixa mais distante dele.
//! EN: Where each inserter picks up and where it drops. The prototype positions are defined with
//!     the inserter facing north and rotated by the blueprint direction, which points to the side
//!     it picks from. On a belt, the drop lane is the one on the drop point's side: since the
//!     point is 0.2 tile past the center, the inserter drops on the lane farthest from it.

use bf_blueprint::Blueprint;
use bf_data::{EntityKind, GameData};
use serde::Serialize;

use crate::World;
use crate::belts::{BeltNetwork, LEFT, RIGHT};
use crate::dir::{Dir, tile_of};

/// PT: Ponta de um insersor. EN: One end of an inserter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Endpoint {
    /// PT: Chão vazio. EN: Empty ground.
    Nothing,
    /// PT: Entidade (máquina, baú...), pela posição em `World::entities`.
    /// EN: Entity (machine, chest...), by position in `World::entities`.
    Entity { entity: usize },
    /// PT: Tile de esteira; `lane` só na entrega. EN: Belt tile; `lane` only when dropping.
    Belt { tile: usize, lane: Option<usize> },
}

/// PT: Um insersor com as duas pontas. EN: One inserter with both ends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InserterLink {
    pub entity: usize,
    pub dir: Dir,
    pub pickup: Endpoint,
    pub drop: Endpoint,
    pub pickup_point: (f64, f64),
    pub drop_point: (f64, f64),
}

/// PT: Liga todos os insersores com direção cardeal. EN: Links every inserter with a cardinal direction.
pub fn link(
    world: &World,
    blueprint: &Blueprint,
    data: &GameData,
    belts: &BeltNetwork,
) -> Vec<InserterLink> {
    let mut links = Vec::new();
    for (slot, placed) in world.entities.iter().enumerate() {
        let Some(EntityKind::Inserter {
            pickup_position,
            insert_position,
            ..
        }) = data.entities.get(&placed.name).map(|p| &p.kind)
        else {
            continue;
        };
        let (Some(dir), Some(source)) = (
            Dir::from_direction(placed.direction),
            blueprint.entities.get(placed.index),
        ) else {
            continue;
        };
        let center = (source.position.x, source.position.y);
        let at = |offset: [f64; 2]| {
            let (dx, dy) = dir.rotate((offset[0], offset[1]));
            (center.0 + dx, center.1 + dy)
        };
        let pickup_point = at(*pickup_position);
        let drop_point = at(*insert_position);
        links.push(InserterLink {
            entity: slot,
            dir,
            pickup: endpoint(world, belts, slot, pickup_point, false),
            drop: endpoint(world, belts, slot, drop_point, true),
            pickup_point,
            drop_point,
        });
    }
    links
}

fn endpoint(
    world: &World,
    belts: &BeltNetwork,
    own: usize,
    point: (f64, f64),
    dropping: bool,
) -> Endpoint {
    let (x, y) = tile_of(point.0, point.1);
    if let Some(tile) = belts.at(x, y) {
        let lane = dropping
            .then(|| belts.tiles.get(tile).map(|t| lane_at(t.dir, x, y, point)))
            .flatten();
        return Endpoint::Belt { tile, lane };
    }
    // PT: Em `World::entities` a posição é o próprio índice da blueprint.
    // EN: In `World::entities` the position is the blueprint index itself.
    match world.entity_at(x, y) {
        Some(entity) if entity.index != own => Endpoint::Entity {
            entity: entity.index,
        },
        _ => Endpoint::Nothing,
    }
}

// PT: Faixa do lado em que o ponto está, olhando no sentido da esteira. Ponto em cima do eixo
//     (insersor alinhado com a esteira) cai na direita, como no jogo.
// EN: Lane on the side the point is on, looking along the belt. A point on the axis (inserter
//     aligned with the belt) goes to the right, as in the game.
fn lane_at(dir: Dir, x: i64, y: i64, point: (f64, f64)) -> usize {
    let offset = (point.0 - (x as f64 + 0.5), point.1 - (y as f64 + 0.5));
    let (rx, ry) = dir.right().vector();
    let side = offset.0 * rx as f64 + offset.1 * ry as f64;
    if side < -1e-6 { LEFT } else { RIGHT }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_on_the_far_lane() {
        // PT: Esteira indo para o leste em (0, 1); insersor ao norte dela solta em y = 1,7.
        // EN: Belt going east at (0, 1); an inserter north of it drops at y = 1.7.
        assert_eq!(lane_at(Dir::East, 0, 1, (0.5, 1.7)), RIGHT);
        // PT: Vindo do sul, solta em y = 1,3: faixa esquerda. EN: From the south it drops at y = 1.3: left lane.
        assert_eq!(lane_at(Dir::East, 0, 1, (0.5, 1.3)), LEFT);
        assert_eq!(lane_at(Dir::East, 0, 1, (0.7, 1.5)), RIGHT);
    }
}
