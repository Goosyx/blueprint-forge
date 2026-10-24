//! PT: Leitura e escrita da blueprint string: `"0" + base64(zlib(JSON))`. A leitura faz as
//!     checagens baratas primeiro (tamanho e alfabeto), descomprime com teto e só então lê o JSON.
//! EN: Reading and writing of the blueprint string: `"0" + base64(zlib(JSON))`. Reading runs the
//!     cheap checks first (size and alphabet), decompresses with a cap and only then reads JSON.

use std::io::{Read, Write};

use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::bounded::take_limit_hit;
use crate::error::{DecodeError, EncodeError};
use crate::legacy;
use crate::limits::Limits;
use crate::model::{BlueprintBook, BlueprintString, Root};
use crate::validate::validate;

/// PT: Base64 padrão, aceitando a string com ou sem `=` no final.
/// EN: Standard base64, accepting the string with or without trailing `=`.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// PT: Avisos da leitura que não impedem a análise.
/// EN: Reading warnings that do not prevent the analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// PT: A blueprint era do 1.1 e foi convertida para o 2.0.
    /// EN: The blueprint was from 1.1 and was converted to 2.0.
    ConvertedFromLegacy,
}

/// PT: Resultado da leitura.
/// EN: Reading result.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub content: BlueprintString,
    pub warnings: Vec<Warning>,
}

/// PT: Confere tamanho, versão e alfabeto sem decodificar nada. Devolve o base64 sem o `0`.
///     Espaços e quebras de linha só são aceitos nas pontas.
/// EN: Checks size, version and alphabet without decoding anything. Returns the base64 without
///     the `0`. Spaces and line breaks are only accepted at the ends.
pub fn precheck<'a>(text: &'a str, limits: &Limits) -> Result<&'a str, DecodeError> {
    if text.len() > limits.max_encoded_len {
        return Err(DecodeError::TooLarge {
            len: text.len(),
            max: limits.max_encoded_len,
        });
    }
    let trimmed = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let mut chars = trimmed.chars();
    match chars.next() {
        None => return Err(DecodeError::Empty),
        Some('0') => {}
        Some(other) => return Err(DecodeError::UnsupportedVersion(other)),
    }
    let body = chars.as_str();
    if let Some(position) = body
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='))
    {
        return Err(DecodeError::InvalidCharacter(position + 1));
    }
    if body.is_empty() {
        return Err(DecodeError::Empty);
    }
    Ok(body)
}

/// PT: Lê uma blueprint string colada pelo usuário, aplicando todos os limites.
/// EN: Reads a blueprint string pasted by the user, applying every limit.
pub fn decode(text: &str, limits: &Limits) -> Result<Decoded, DecodeError> {
    let body = precheck(text, limits)?;
    let compressed = BASE64
        .decode(body)
        .map_err(|_| DecodeError::InvalidBase64)?;
    let json = inflate(&compressed, limits)?;
    let json = std::str::from_utf8(&json).map_err(|_| DecodeError::InvalidUtf8)?;
    if crate::model::json_depth(json) > crate::limits::HARD_MAX_JSON_DEPTH {
        return Err(DecodeError::InvalidJson {
            category: "depth",
            line: 0,
            column: 0,
        });
    }

    // PT: Limpa um registro de limite que tenha sobrado de outra leitura nesta thread.
    // EN: Clears any limit record left over from another read on this thread.
    take_limit_hit();
    let root: Root = serde_json::from_str(json).map_err(map_json_error)?;
    let mut content = match root {
        Root {
            blueprint: Some(blueprint),
            blueprint_book: None,
        } => BlueprintString::Blueprint(blueprint),
        Root {
            blueprint: None,
            blueprint_book: Some(book),
        } => BlueprintString::Book(book),
        _ => return Err(DecodeError::NotABlueprint),
    };

    validate(&content, limits)?;

    let mut warnings = Vec::new();
    if upgrade_legacy(&mut content)? {
        warnings.push(Warning::ConvertedFromLegacy);
    }
    // PT: Confere de novo: a conversão cria fios e muda direções.
    // EN: Checks again: the conversion creates wires and changes directions.
    validate(&content, limits)?;
    Ok(Decoded { content, warnings })
}

/// PT: Gera a blueprint string a partir do modelo.
/// EN: Produces the blueprint string from the model.
pub fn encode(content: &BlueprintString) -> Result<String, EncodeError> {
    let root = match content {
        BlueprintString::Blueprint(blueprint) => Root {
            blueprint: Some(blueprint.clone()),
            blueprint_book: None,
        },
        BlueprintString::Book(book) => Root {
            blueprint: None,
            blueprint_book: Some(book.clone()),
        },
    };
    let json = serde_json::to_vec(&root)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&json)?;
    let compressed = encoder.finish()?;
    Ok(format!("0{}", BASE64.encode(compressed)))
}

/// PT: Comprime um JSON qualquer no formato da blueprint string, sem validar. Útil para montar
///     cenários de teste; o resultado deve passar por `decode` antes de qualquer uso.
/// EN: Compresses arbitrary JSON into the blueprint string format, without validating. Useful
///     for building test scenarios; the result must go through `decode` before any use.
pub fn encode_raw_json(json: &str) -> Result<String, EncodeError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(json.as_bytes())?;
    Ok(format!("0{}", BASE64.encode(encoder.finish()?)))
}

// PT: Descomprime com teto de tamanho e de razão. Lê no máximo `max + 1` bytes para saber se passou.
// EN: Decompresses with a size cap and a ratio cap. Reads at most `max + 1` bytes to know if it overflowed.
fn inflate(compressed: &[u8], limits: &Limits) -> Result<Vec<u8>, DecodeError> {
    let max = limits.max_decoded_len;
    let cap = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    let mut output = Vec::new();
    ZlibDecoder::new(compressed)
        .take(cap)
        .read_to_end(&mut output)
        .map_err(|_| DecodeError::InvalidCompression)?;
    if output.len() > max {
        return Err(DecodeError::DecompressedTooLarge { max });
    }
    if output.len()
        > compressed
            .len()
            .saturating_mul(limits.max_compression_ratio)
    {
        return Err(DecodeError::CompressionRatio);
    }
    Ok(output)
}

// PT: Limite registrado durante a leitura vira `LimitExceeded`; o resto vira `InvalidJson` só com
//     categoria, linha e coluna (a mensagem do serde pode repetir texto do usuário).
// EN: A limit recorded during reading becomes `LimitExceeded`; the rest becomes `InvalidJson` with
//     only category, line and column (the serde message may repeat user text).
fn map_json_error(error: serde_json::Error) -> DecodeError {
    if let Some(max) = take_limit_hit() {
        return DecodeError::LimitExceeded {
            what: "list",
            found: max.saturating_add(1),
            max,
        };
    }
    let category = match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    DecodeError::InvalidJson {
        category,
        line: error.line(),
        column: error.column(),
    }
}

// PT: Converte todas as blueprints do 1.1, inclusive dentro de livros.
// EN: Converts every 1.1 blueprint, including those inside books.
fn upgrade_legacy(content: &mut BlueprintString) -> Result<bool, DecodeError> {
    match content {
        BlueprintString::Blueprint(blueprint) => legacy::upgrade(blueprint),
        BlueprintString::Book(book) => {
            let mut changed = false;
            let mut stack: Vec<&mut BlueprintBook> = vec![book];
            while let Some(book) = stack.pop() {
                for entry in &mut book.blueprints {
                    if let Some(blueprint) = &mut entry.blueprint {
                        changed |= legacy::upgrade(blueprint)?;
                    }
                    if let Some(child) = &mut entry.blueprint_book {
                        stack.push(child);
                    }
                }
            }
            Ok(changed)
        }
    }
}
