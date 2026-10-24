//! PT: Validação depois da leitura: limites de quantidade, profundidade de livros, posições,
//!     números de entidade, direções, fios e tamanho dos textos.
//! EN: Post-read validation: count limits, book depth, positions, entity numbers, directions,
//!     wires and text sizes.

use std::collections::HashSet;

use crate::error::DecodeError;
use crate::limits::Limits;
use crate::model::{Blueprint, BlueprintBook, BlueprintString, ItemRequests};

/// PT: Conectores de fio do 2.0: circuito 1 a 4 (vermelho/verde, entrada/saída) e cobre 5 e 6.
/// EN: 2.0 wire connectors: circuit 1 to 4 (red/green, input/output) and copper 5 and 6.
const VALID_CONNECTORS: std::ops::RangeInclusive<u64> = 1..=6;

/// PT: Confere tudo que não foi conferido durante a desserialização.
/// EN: Checks everything that was not checked during deserialization.
pub fn validate(content: &BlueprintString, limits: &Limits) -> Result<(), DecodeError> {
    match content {
        BlueprintString::Blueprint(blueprint) => validate_blueprint(blueprint, limits),
        BlueprintString::Book(book) => validate_book(book, limits),
    }
}

// PT: Percorre o livro com pilha explícita, contando profundidade, blueprints e entidades.
// EN: Walks the book with an explicit stack, counting depth, blueprints and entities.
fn validate_book(root: &BlueprintBook, limits: &Limits) -> Result<(), DecodeError> {
    let mut stack: Vec<(&BlueprintBook, usize)> = vec![(root, 1)];
    let mut total_blueprints = 0usize;
    let mut total_entities = 0usize;

    while let Some((book, depth)) = stack.pop() {
        if depth > limits.max_book_depth {
            return Err(DecodeError::BookTooDeep {
                max: limits.max_book_depth,
            });
        }
        check_count(
            "book_entries",
            book.blueprints.len(),
            limits.max_book_entries,
        )?;
        check_text("label", book.label.as_deref(), limits.max_label_chars)?;
        check_text(
            "description",
            book.description.as_deref(),
            limits.max_description_chars,
        )?;

        for entry in &book.blueprints {
            if let Some(blueprint) = &entry.blueprint {
                total_blueprints += 1;
                total_entities += blueprint.entities.len();
                check_count(
                    "total_blueprints",
                    total_blueprints,
                    limits.max_total_blueprints,
                )?;
                check_count("total_entities", total_entities, limits.max_total_entities)?;
                validate_blueprint(blueprint, limits)?;
            }
            if let Some(child) = &entry.blueprint_book {
                stack.push((child, depth + 1));
            }
        }
    }
    Ok(())
}

fn validate_blueprint(blueprint: &Blueprint, limits: &Limits) -> Result<(), DecodeError> {
    check_count("entities", blueprint.entities.len(), limits.max_entities)?;
    check_count("tiles", blueprint.tiles.len(), limits.max_tiles)?;
    check_count("wires", blueprint.wires.len(), limits.max_wires)?;
    check_text("label", blueprint.label.as_deref(), limits.max_label_chars)?;
    check_text(
        "description",
        blueprint.description.as_deref(),
        limits.max_description_chars,
    )?;

    // PT: Número de entidade máximo aceito: o dobro do limite de entidades.
    // EN: Highest accepted entity number: twice the entity limit.
    let max_number = u64::try_from(limits.max_entities.saturating_mul(2)).unwrap_or(u64::MAX);
    let mut numbers = HashSet::with_capacity(blueprint.entities.len());
    let mut bounds: Option<(f64, f64, f64, f64)> = None;

    for entity in &blueprint.entities {
        let number = entity.entity_number;
        if number == 0 || number > max_number {
            return Err(DecodeError::InvalidEntityNumber(number));
        }
        if !numbers.insert(number) {
            return Err(DecodeError::DuplicateEntityNumber(number));
        }
        for name in [
            Some(entity.name.as_str()),
            entity.recipe.as_deref(),
            entity.quality.as_deref(),
            entity.recipe_quality.as_deref(),
            entity.io_type.as_deref(),
        ] {
            check_text("name", name, limits.max_name_chars)?;
        }
        if let Some(ItemRequests::Modern(requests)) = &entity.items {
            for request in requests {
                check_text("name", Some(&request.id.name), limits.max_name_chars)?;
                check_text("name", request.id.quality.as_deref(), limits.max_name_chars)?;
            }
        }
        if let Some(ItemRequests::Legacy(counts)) = &entity.items {
            for name in counts.keys() {
                check_text("name", Some(name), limits.max_name_chars)?;
            }
        }
        if entity.direction > 15 {
            return Err(DecodeError::InvalidDirection {
                entity_number: number,
            });
        }

        let (x, y) = (entity.position.x, entity.position.y);
        if !valid_coordinate(x, limits) || !valid_coordinate(y, limits) {
            return Err(DecodeError::InvalidPosition {
                entity_number: number,
            });
        }
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
        });
    }

    for tile in &blueprint.tiles {
        check_text("name", Some(&tile.name), limits.max_name_chars)?;
        let (x, y) = (tile.position.x, tile.position.y);
        if !valid_coordinate(x, limits) || !valid_coordinate(y, limits) {
            return Err(DecodeError::InvalidPosition { entity_number: 0 });
        }
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
        });
    }
    for icon in &blueprint.icons {
        check_text("name", Some(&icon.signal.name), limits.max_name_chars)?;
    }

    if let Some((min_x, min_y, max_x, max_y)) = bounds {
        let area = (max_x - min_x + 1.0) * (max_y - min_y + 1.0);
        if area > limits.max_bounding_area {
            return Err(DecodeError::AreaTooLarge);
        }
    }

    for wire in &blueprint.wires {
        let [a, connector_a, b, connector_b] = *wire;
        if !VALID_CONNECTORS.contains(&connector_a) || !VALID_CONNECTORS.contains(&connector_b) {
            return Err(DecodeError::InvalidWire);
        }
        for end in [a, b] {
            if !numbers.contains(&end) {
                return Err(DecodeError::DanglingWire(end));
            }
        }
    }
    Ok(())
}

// PT: Coordenada finita e dentro do limite. Não exige múltiplo de 0,5: vagões e carros podem
//     ficar em posições quebradas, e isso não é perigoso.
// EN: Finite coordinate within the limit. It does not require a multiple of 0.5: wagons and
//     cars may sit at fractional positions, and that is not dangerous.
fn valid_coordinate(value: f64, limits: &Limits) -> bool {
    value.is_finite() && value.abs() <= limits.max_coordinate
}

fn check_count(what: &'static str, found: usize, max: usize) -> Result<(), DecodeError> {
    if found > max {
        return Err(DecodeError::LimitExceeded { what, found, max });
    }
    Ok(())
}

fn check_text(what: &'static str, text: Option<&str>, max_chars: usize) -> Result<(), DecodeError> {
    if let Some(text) = text
        && text.chars().count() > max_chars
    {
        return Err(DecodeError::TextTooLong { what });
    }
    Ok(())
}
