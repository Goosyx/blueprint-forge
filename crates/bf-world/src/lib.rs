//! PT: Constrói o "mundo" de uma blueprint: cada entidade com sua área na grade, a grade de
//!     ocupação esparsa e os avisos (entidade desconhecida, sobreposição). A grade nunca é uma
//!     matriz densa: guarda só os tiles ocupados, com teto de quantidade.
//! EN: Builds the "world" of a blueprint: each entity with its grid area, the sparse occupancy
//!     grid and the warnings (unknown entity, overlap). The grid is never a dense matrix: it only
//!     keeps occupied tiles, with a count cap.

pub mod belts;
pub mod dir;
pub mod fluids;
pub mod footprint;
pub mod inserters;
pub mod markers;
pub mod power;
mod union;

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use bf_blueprint::Blueprint;
use bf_data::{GameData, TileSize};
use serde::Serialize;

pub use footprint::Footprint;

/// PT: Teto de tiles ocupados que a grade aceita (evita estourar memória com entidades enormes).
/// EN: Cap on occupied tiles the grid accepts (avoids blowing memory with huge entities).
pub const MAX_OCCUPIED_TILES: u64 = 2_000_000;

/// PT: Quantidade máxima de avisos guardados; o resto só é contado.
/// EN: Maximum number of stored warnings; the rest are only counted.
pub const MAX_WARNINGS: usize = 200;

/// PT: Tipos de protótipo que podem se sobrepor no jogo (trilhos e sinais).
/// EN: Prototype types that may overlap in the game (rails and signals).
const OVERLAP_ALLOWED: &[&str] = &[
    "straight-rail",
    "curved-rail-a",
    "curved-rail-b",
    "half-diagonal-rail",
    "legacy-straight-rail",
    "legacy-curved-rail",
    "rail-signal",
    "rail-chain-signal",
    "land-mine",
];

/// PT: Entidade posicionada na grade.
/// EN: Entity placed on the grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedEntity {
    /// PT: Posição na lista de entidades da blueprint.
    /// EN: Position in the blueprint's entity list.
    pub index: usize,
    pub entity_number: u64,
    pub name: String,
    /// PT: Tipo do protótipo; `None` quando a entidade não existe nos dados do jogo.
    /// EN: Prototype type; `None` when the entity does not exist in the game data.
    pub prototype: Option<String>,
    pub direction: u8,
    pub footprint: Footprint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
}

/// PT: Aviso da construção do mundo.
/// EN: World construction warning.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum WorldWarning {
    /// PT: Entidade fora dos dados do jogo (mod ou nome errado); desenhada como 1×1.
    /// EN: Entity missing from the game data (mod or wrong name); drawn as 1×1.
    UnknownEntity { entity_number: u64, name: String },
    /// PT: Duas entidades ocupam o mesmo tile.
    /// EN: Two entities take the same tile.
    Overlap {
        first: u64,
        second: u64,
        x: i64,
        y: i64,
    },
}

/// PT: Erro que impede montar o mundo.
/// EN: Error that prevents building the world.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorldError {
    #[error("tiles ocupados demais / too many occupied tiles (> {max})")]
    TooManyTiles { max: u64 },
}

impl WorldError {
    /// PT: Código estável do erro, usado como chave de tradução.
    /// EN: Stable error code, used as the translation key.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooManyTiles { .. } => "world.too_many_tiles",
        }
    }
}

/// PT: Caixa envolvente em tiles (inclusiva no canto superior esquerdo, exclusiva no outro).
/// EN: Bounding box in tiles (inclusive at the top-left corner, exclusive at the other).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bounds {
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

/// PT: Mundo de uma blueprint.
/// EN: World of a blueprint.
#[derive(Debug, Clone, Serialize)]
pub struct World {
    pub entities: Vec<PlacedEntity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    pub warnings: Vec<WorldWarning>,
    /// PT: Avisos além de `MAX_WARNINGS`, só contados.
    /// EN: Warnings beyond `MAX_WARNINGS`, only counted.
    pub dropped_warnings: usize,
    #[serde(skip)]
    /// PT: Tile → posição em `entities`. Chaves e valores de 32 bits para caber ~20 bytes por tile.
    /// EN: Tile → position in `entities`. 32-bit keys and values to fit ~20 bytes per tile.
    occupancy: HashMap<(i32, i32), u32>,
}

/// PT: Topologia completa: esteiras, insersores, energia e fluidos, mais os índices que as
///     simulações e a dedução de fornalhas usam (montados uma vez, em custo linear).
/// EN: Full topology: belts, inserters, power and fluids, plus the indexes the simulations and
///     the furnace inference use (built once, at linear cost).
#[derive(Debug, Clone, Serialize)]
pub struct Topology {
    pub belts: belts::BeltNetwork,
    pub inserters: Vec<inserters::InserterLink>,
    pub power: power::PowerCoverage,
    pub fluids: fluids::FluidNetworks,
    /// PT: Cabeça de esteira → itens marcados por combinador. EN: Belt head → items marked by a combinator.
    #[serde(skip)]
    pub markers: std::collections::BTreeMap<usize, Vec<String>>,
    /// PT: Ordem das esteiras de quem manda para quem recebe. EN: Belt order from senders to receivers.
    #[serde(skip)]
    pub order: Vec<usize>,
    /// PT: Tile → insersores que pegam nele. EN: Tile → inserters picking from it.
    #[serde(skip)]
    pub pickers_by_tile: HashMap<usize, Vec<usize>>,
    /// PT: Tile → insersores que soltam nele. EN: Tile → inserters dropping onto it.
    #[serde(skip)]
    pub droppers_by_tile: HashMap<usize, Vec<usize>>,
    /// PT: Entidade → insersores que soltam nela. EN: Entity → inserters dropping into it.
    #[serde(skip)]
    pub feeders_of: HashMap<usize, Vec<usize>>,
    /// PT: Entidade → insersores que pegam dela. EN: Entity → inserters picking from it.
    #[serde(skip)]
    pub takers_of: HashMap<usize, Vec<usize>>,
}

impl Topology {
    /// PT: Monta a topologia do mundo. Custo linear no número de entidades.
    /// EN: Builds the world's topology. Linear cost in the number of entities.
    pub fn build(world: &World, blueprint: &Blueprint, data: &GameData) -> Self {
        let belts = belts::BeltNetwork::build(world, blueprint, data);
        let inserters = inserters::link(world, blueprint, data, &belts);
        let mut pickers_by_tile: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut droppers_by_tile: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut feeders_of: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut takers_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for (k, link) in inserters.iter().enumerate() {
            match link.pickup {
                inserters::Endpoint::Belt { tile, .. } => {
                    pickers_by_tile.entry(tile).or_default().push(k)
                }
                inserters::Endpoint::Entity { entity } => {
                    takers_of.entry(entity).or_default().push(k)
                }
                inserters::Endpoint::Nothing => {}
            }
            match link.drop {
                inserters::Endpoint::Belt { tile, .. } => {
                    droppers_by_tile.entry(tile).or_default().push(k)
                }
                inserters::Endpoint::Entity { entity } => {
                    feeders_of.entry(entity).or_default().push(k)
                }
                inserters::Endpoint::Nothing => {}
            }
        }
        Self {
            markers: markers::marked_heads(world, blueprint, data, &belts),
            order: belts.order(),
            inserters,
            power: power::PowerCoverage::build(world, blueprint, data),
            fluids: fluids::FluidNetworks::build(world, blueprint, data),
            belts,
            pickers_by_tile,
            droppers_by_tile,
            feeders_of,
            takers_of,
        }
    }
}

impl World {
    /// PT: Monta o mundo a partir de uma blueprint já validada pelo codec.
    /// EN: Builds the world from a blueprint already validated by the codec.
    pub fn build(blueprint: &Blueprint, data: &GameData) -> Result<Self, WorldError> {
        let mut world = Self {
            entities: Vec::with_capacity(blueprint.entities.len()),
            bounds: None,
            warnings: Vec::new(),
            dropped_warnings: 0,
            occupancy: HashMap::new(),
        };

        // PT: Confere o total de tiles antes de inserir qualquer um.
        // EN: Checks the total number of tiles before inserting any.
        let mut total_tiles: u64 = 0;
        let mut placed = Vec::with_capacity(blueprint.entities.len());
        for (index, entity) in blueprint.entities.iter().enumerate() {
            let prototype = data.entities.get(&entity.name);
            let size = prototype.map_or(
                TileSize {
                    width: 1,
                    height: 1,
                },
                |p| p.size,
            );
            let footprint = Footprint::from_center(
                entity.position.x,
                entity.position.y,
                size,
                entity.direction,
            );
            total_tiles = total_tiles.saturating_add(footprint.area());
            if total_tiles > MAX_OCCUPIED_TILES {
                return Err(WorldError::TooManyTiles {
                    max: MAX_OCCUPIED_TILES,
                });
            }
            if prototype.is_none() {
                world.warn(WorldWarning::UnknownEntity {
                    entity_number: entity.entity_number,
                    name: entity.name.clone(),
                });
            }
            placed.push(PlacedEntity {
                index,
                entity_number: entity.entity_number,
                name: entity.name.clone(),
                prototype: prototype.map(|p| p.prototype.clone()),
                direction: entity.direction,
                footprint,
                recipe: entity.recipe.clone(),
            });
        }

        for (slot, entity) in placed.iter().enumerate() {
            world.extend_bounds(&entity.footprint);
            let may_overlap = entity
                .prototype
                .as_deref()
                .is_some_and(|p| OVERLAP_ALLOWED.contains(&p));
            if may_overlap {
                continue;
            }
            let Ok(slot32) = u32::try_from(slot) else {
                continue;
            };
            for (x, y) in entity.footprint.tiles() {
                // PT: O codec limita as coordenadas a ±100.000, então cabem em i32.
                // EN: The codec limits coordinates to ±100,000, so they fit in i32.
                let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
                    continue;
                };
                match world.occupancy.entry((x, y)) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(slot32);
                    }
                    Entry::Occupied(occupied) => {
                        let first = usize::try_from(*occupied.get())
                            .ok()
                            .and_then(|index| placed.get(index))
                            .map_or(0, |e| e.entity_number);
                        world.warn(WorldWarning::Overlap {
                            first,
                            second: entity.entity_number,
                            x: i64::from(x),
                            y: i64::from(y),
                        });
                    }
                }
            }
        }

        world.entities = placed;
        Ok(world)
    }

    /// PT: Entidade que ocupa o tile, se houver (trilhos e sinais não entram na grade).
    /// EN: Entity occupying the tile, if any (rails and signals are not in the grid).
    pub fn entity_at(&self, x: i64, y: i64) -> Option<&PlacedEntity> {
        let key = (i32::try_from(x).ok()?, i32::try_from(y).ok()?);
        let slot = usize::try_from(*self.occupancy.get(&key)?).ok()?;
        self.entities.get(slot)
    }

    fn warn(&mut self, warning: WorldWarning) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(warning);
        } else {
            self.dropped_warnings += 1;
        }
    }

    fn extend_bounds(&mut self, footprint: &Footprint) {
        let right = footprint.x + i64::from(footprint.width);
        let bottom = footprint.y + i64::from(footprint.height);
        self.bounds = Some(match self.bounds {
            None => Bounds {
                min_x: footprint.x,
                min_y: footprint.y,
                max_x: right,
                max_y: bottom,
            },
            Some(bounds) => Bounds {
                min_x: bounds.min_x.min(footprint.x),
                min_y: bounds.min_y.min(footprint.y),
                max_x: bounds.max_x.max(right),
                max_y: bounds.max_y.max(bottom),
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bf_blueprint::{Entity, Position};
    use bf_data::{Entity as Proto, EntityKind};
    use std::collections::BTreeMap;

    fn data() -> GameData {
        let mut entities = BTreeMap::new();
        let mut add = |name: &str, prototype: &str, width: u32, height: u32| {
            entities.insert(
                name.to_owned(),
                Proto {
                    name: name.to_owned(),
                    prototype: prototype.to_owned(),
                    size: TileSize { width, height },
                    collision_box: [[0.0, 0.0], [0.0, 0.0]],
                    placed_by: Vec::new(),
                    energy_source: None,
                    energy_usage_w: None,
                    fluid_boxes: Vec::new(),
                    kind: EntityKind::Other,
                },
            );
        };
        add("assembling-machine-2", "assembling-machine", 3, 3);
        add("inserter", "inserter", 1, 1);
        add("straight-rail", "straight-rail", 2, 2);
        GameData {
            format_version: bf_data::FORMAT_VERSION,
            game_version: "2.1.23".into(),
            items: BTreeMap::new(),
            fluids: BTreeMap::new(),
            recipes: BTreeMap::new(),
            entities,
            tiles: BTreeMap::new(),
        }
    }

    fn blueprint(entities: Vec<Entity>) -> Blueprint {
        Blueprint {
            entities,
            ..Blueprint::default()
        }
    }

    #[test]
    fn places_entities_and_computes_bounds() {
        let world = World::build(
            &blueprint(vec![
                Entity::new(1, "assembling-machine-2", Position { x: 1.5, y: 1.5 }),
                Entity::new(2, "inserter", Position { x: 1.5, y: 3.5 }),
            ]),
            &data(),
        )
        .unwrap();
        assert!(world.warnings.is_empty());
        assert_eq!(world.entity_at(2, 2).unwrap().entity_number, 1);
        assert_eq!(world.entity_at(1, 3).unwrap().entity_number, 2);
        assert!(world.entity_at(5, 5).is_none());
        assert_eq!(
            world.bounds,
            Some(Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 3,
                max_y: 4
            })
        );
    }

    #[test]
    fn warns_about_unknown_and_overlapping_entities() {
        let world = World::build(
            &blueprint(vec![
                Entity::new(1, "assembling-machine-2", Position { x: 1.5, y: 1.5 }),
                Entity::new(2, "inserter", Position { x: 0.5, y: 0.5 }),
                Entity::new(3, "modded-thing", Position { x: 10.5, y: 10.5 }),
                Entity::new(4, "straight-rail", Position { x: 1.0, y: 1.0 }),
            ]),
            &data(),
        )
        .unwrap();
        assert!(world.warnings.contains(&WorldWarning::Overlap {
            first: 1,
            second: 2,
            x: 0,
            y: 0
        }));
        assert!(world.warnings.contains(&WorldWarning::UnknownEntity {
            entity_number: 3,
            name: "modded-thing".into()
        }));
        assert_eq!(
            world.warnings.len(),
            2,
            "trilho pode sobrepor / rails may overlap"
        );
    }

    #[test]
    fn rejects_too_many_tiles() {
        let mut entities = Vec::new();
        // PT: 500 mil máquinas 3×3 dão 4,5 milhões de tiles. EN: 500k 3×3 machines give 4.5M tiles.
        for number in 1..=500_000u64 {
            entities.push(Entity::new(
                number,
                "assembling-machine-2",
                Position {
                    x: 1.5 + 3.0 * (number % 1000) as f64,
                    y: 1.5 + 3.0 * (number / 1000) as f64,
                },
            ));
        }
        let error = World::build(&blueprint(entities), &data()).unwrap_err();
        assert_eq!(error.code(), "world.too_many_tiles");
    }
}
