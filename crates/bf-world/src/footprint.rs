//! PT: Área ocupada por uma entidade na grade: tamanho virado conforme a direção e canto
//!     superior esquerdo calculado a partir do centro gravado na blueprint.
//! EN: Area an entity takes on the grid: size rotated by direction and top-left corner computed
//!     from the center stored in the blueprint.

use bf_data::TileSize;
use serde::Serialize;

/// PT: Retângulo de tiles: canto superior esquerdo e tamanho.
/// EN: Tile rectangle: top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Footprint {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

impl Footprint {
    /// PT: Calcula a área a partir do centro, do tamanho virado para o norte e da direção (16 passos).
    ///     Leste e oeste trocam largura e altura. O epsilon evita erro de arredondamento do float.
    /// EN: Computes the area from the center, the north-facing size and the direction (16 steps).
    ///     East and west swap width and height. The epsilon avoids float rounding errors.
    pub fn from_center(center_x: f64, center_y: f64, size: TileSize, direction: u8) -> Self {
        let (width, height) = if is_sideways(direction) {
            (size.height, size.width)
        } else {
            (size.width, size.height)
        };
        let left = center_x - f64::from(width) / 2.0 + 1e-6;
        let top = center_y - f64::from(height) / 2.0 + 1e-6;
        Self {
            x: left.floor() as i64,
            y: top.floor() as i64,
            width,
            height,
        }
    }

    /// PT: Todos os tiles cobertos, linha por linha.
    /// EN: Every covered tile, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        (0..i64::from(self.height)).flat_map(move |dy| {
            (0..i64::from(self.width)).map(move |dx| (self.x + dx, self.y + dy))
        })
    }

    /// PT: Quantidade de tiles cobertos.
    /// EN: Number of covered tiles.
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// PT: Leste (4) e oeste (12) deitam a entidade. Direções diagonais não trocam o tamanho.
/// EN: East (4) and west (12) turn the entity sideways. Diagonal directions do not swap the size.
pub fn is_sideways(direction: u8) -> bool {
    direction.is_multiple_of(4) && !(direction / 4).is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn size(width: u32, height: u32) -> TileSize {
        TileSize { width, height }
    }

    #[test]
    fn odd_sizes_have_half_tile_centers() {
        let machine = Footprint::from_center(1.5, 1.5, size(3, 3), 0);
        assert_eq!(
            machine,
            Footprint {
                x: 0,
                y: 0,
                width: 3,
                height: 3
            }
        );
        let refinery = Footprint::from_center(-2.5, 2.5, size(5, 5), 0);
        assert_eq!(
            refinery,
            Footprint {
                x: -5,
                y: 0,
                width: 5,
                height: 5
            }
        );
    }

    #[test]
    fn even_sizes_have_integer_centers() {
        let furnace = Footprint::from_center(1.0, 1.0, size(2, 2), 0);
        assert_eq!(
            furnace,
            Footprint {
                x: 0,
                y: 0,
                width: 2,
                height: 2
            }
        );
    }

    #[test]
    fn sideways_entities_swap_size() {
        // PT: Bomba 1×2 virada para leste ocupa 2×1. EN: A 1×2 pump facing east takes 2×1.
        let pump = Footprint::from_center(1.0, 0.5, size(1, 2), 4);
        assert_eq!(
            pump,
            Footprint {
                x: 0,
                y: 0,
                width: 2,
                height: 1
            }
        );
        let boiler_west = Footprint::from_center(1.0, 1.5, size(3, 2), 12);
        assert_eq!(
            boiler_west,
            Footprint {
                x: 0,
                y: 0,
                width: 2,
                height: 3
            }
        );
        let boiler_south = Footprint::from_center(1.5, 1.0, size(3, 2), 8);
        assert_eq!(
            boiler_south,
            Footprint {
                x: 0,
                y: 0,
                width: 3,
                height: 2
            }
        );
    }

    #[test]
    fn lists_covered_tiles() {
        let footprint = Footprint {
            x: -1,
            y: 2,
            width: 2,
            height: 1,
        };
        assert_eq!(footprint.tiles().collect::<Vec<_>>(), vec![(-1, 2), (0, 2)]);
        assert_eq!(footprint.area(), 2);
    }
}
