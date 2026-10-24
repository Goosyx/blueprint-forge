//! PT: Conversão das unidades de energia e potência usadas nos protótipos do jogo.
//! EN: Conversion of the energy and power units used by the game's prototypes.

use thiserror::Error;

/// PT: Erro ao interpretar um valor como `"150kW"` ou `"4MJ"`.
/// EN: Error while parsing a value such as `"150kW"` or `"4MJ"`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UnitError {
    #[error("unidade inválida / invalid unit in {0:?}")]
    InvalidUnit(String),
    #[error("número inválido / invalid number in {0:?}")]
    InvalidNumber(String),
}

/// PT: Grandeza esperada: potência em watts (`W`) ou energia em joules (`J`).
/// EN: Expected quantity: power in watts (`W`) or energy in joules (`J`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    Power,
    Energy,
}

/// PT: Converte `"150kW"` em `150000.0` (W) ou `"4MJ"` em `4000000.0` (J).
///     Aceita os prefixos do SI que o jogo usa, de `k` a `Q`, e recusa a grandeza errada.
/// EN: Converts `"150kW"` into `150000.0` (W) or `"4MJ"` into `4000000.0` (J).
///     Accepts the SI prefixes the game uses, from `k` to `Q`, and rejects the wrong quantity.
pub fn parse_si(text: &str, quantity: Quantity) -> Result<f64, UnitError> {
    let trimmed = text.trim();
    let unit = match quantity {
        Quantity::Power => 'W',
        Quantity::Energy => 'J',
    };
    let without_unit = trimmed
        .strip_suffix(unit)
        .ok_or_else(|| UnitError::InvalidUnit(text.to_owned()))?;

    let (number, multiplier) = match without_unit.chars().last() {
        Some(last) if last.is_ascii_alphabetic() => {
            let multiplier =
                prefix_multiplier(last).ok_or_else(|| UnitError::InvalidUnit(text.to_owned()))?;
            let cut = without_unit.len() - last.len_utf8();
            (without_unit.get(..cut).unwrap_or_default(), multiplier)
        }
        _ => (without_unit, 1.0),
    };

    let value: f64 = number
        .trim()
        .parse()
        .map_err(|_| UnitError::InvalidNumber(text.to_owned()))?;
    if !value.is_finite() || value < 0.0 {
        return Err(UnitError::InvalidNumber(text.to_owned()));
    }
    Ok(value * multiplier)
}

// PT: Multiplicador de cada prefixo do SI aceito pelo jogo.
// EN: Multiplier of each SI prefix accepted by the game.
fn prefix_multiplier(prefix: char) -> Option<f64> {
    let exponent = match prefix {
        'k' | 'K' => 3,
        'M' => 6,
        'G' => 9,
        'T' => 12,
        'P' => 15,
        'E' => 18,
        'Z' => 21,
        'Y' => 24,
        'R' => 27,
        'Q' => 30,
        _ => return None,
    };
    Some(10f64.powi(exponent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_power_and_energy() {
        assert_eq!(parse_si("150kW", Quantity::Power), Ok(150_000.0));
        assert_eq!(parse_si("0.4kW", Quantity::Power), Ok(400.0));
        assert_eq!(parse_si("4MJ", Quantity::Energy), Ok(4_000_000.0));
        assert_eq!(parse_si("5kJ", Quantity::Energy), Ok(5_000.0));
        assert_eq!(parse_si("10GW", Quantity::Power), Ok(10_000_000_000.0));
        assert_eq!(parse_si("60W", Quantity::Power), Ok(60.0));
    }

    #[test]
    fn rejects_wrong_or_invalid_units() {
        assert!(parse_si("150kJ", Quantity::Power).is_err());
        assert!(parse_si("150", Quantity::Power).is_err());
        assert!(parse_si("150xW", Quantity::Power).is_err());
        assert!(parse_si("abckW", Quantity::Power).is_err());
        assert!(parse_si("-1kW", Quantity::Power).is_err());
        assert!(parse_si("infkW", Quantity::Power).is_err());
        assert!(parse_si("", Quantity::Energy).is_err());
    }
}
