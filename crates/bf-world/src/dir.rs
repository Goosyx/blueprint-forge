//! PT: As quatro direções cardeais e as contas de vizinhança na grade (y cresce para o sul).
//! EN: The four cardinal directions and grid neighbourhood math (y grows southwards).

use serde::Serialize;

/// PT: Direção cardeal. EN: Cardinal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    /// PT: Converte a direção de 16 passos da blueprint; diagonais não são cardeais.
    /// EN: Converts the blueprint's 16-step direction; diagonals are not cardinal.
    pub fn from_direction(direction: u8) -> Option<Self> {
        match direction {
            0 => Some(Self::North),
            4 => Some(Self::East),
            8 => Some(Self::South),
            12 => Some(Self::West),
            _ => None,
        }
    }

    /// PT: Passo de um tile nesta direção. EN: One-tile step in this direction.
    pub fn vector(self) -> (i64, i64) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    /// PT: 90° no sentido horário. EN: 90° clockwise.
    pub fn right(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    /// PT: 90° no sentido anti-horário. EN: 90° counter-clockwise.
    pub fn left(self) -> Self {
        self.right().opposite()
    }

    /// PT: Gira um vetor definido para o norte até esta direção.
    /// EN: Rotates a north-defined vector to this direction.
    pub fn rotate(self, (x, y): (f64, f64)) -> (f64, f64) {
        match self {
            Self::North => (x, y),
            Self::East => (-y, x),
            Self::South => (-x, -y),
            Self::West => (y, -x),
        }
    }
}

/// PT: Tile vizinho na direção. EN: Neighbouring tile in the direction.
pub fn step((x, y): (i64, i64), dir: Dir, count: i64) -> (i64, i64) {
    let (dx, dy) = dir.vector();
    (x + dx * count, y + dy * count)
}

/// PT: Tile que contém o ponto. EN: Tile containing the point.
pub fn tile_of(x: f64, y: f64) -> (i64, i64) {
    (x.floor() as i64, y.floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_are_clockwise() {
        assert_eq!(Dir::North.right(), Dir::East);
        assert_eq!(Dir::North.left(), Dir::West);
        assert_eq!(Dir::East.rotate((0.0, -1.0)), (1.0, 0.0));
        assert_eq!(Dir::South.rotate((0.0, -1.0)), (0.0, 1.0));
        assert_eq!(Dir::West.rotate((0.0, -1.0)), (-1.0, 0.0));
        assert_eq!(step((0, 0), Dir::West, 3), (-3, 0));
    }
}
