//! PT: Erros de leitura e escrita. Cada erro tem um `code()` estável, usado como chave de
//!     tradução na interface e no corpo de erro da API.
//! EN: Reading and writing errors. Each error has a stable `code()`, used as the translation
//!     key in the interface and in the API error body.

use thiserror::Error;

/// PT: Por que uma blueprint string foi recusada.
/// EN: Why a blueprint string was rejected.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DecodeError {
    #[error("texto vazio / empty text")]
    Empty,
    #[error("texto grande demais / text too large ({len} > {max} bytes)")]
    TooLarge { len: usize, max: usize },
    #[error("versão da string não suportada / unsupported string version {0:?}")]
    UnsupportedVersion(char),
    #[error("caractere inválido / invalid character at byte {0}")]
    InvalidCharacter(usize),
    #[error("base64 inválido / invalid base64")]
    InvalidBase64,
    #[error("dados comprimidos inválidos / invalid compressed data")]
    InvalidCompression,
    #[error(
        "conteúdo descomprimido grande demais / decompressed content too large (> {max} bytes)"
    )]
    DecompressedTooLarge { max: usize },
    #[error("razão de compressão suspeita / suspicious compression ratio")]
    CompressionRatio,
    #[error("texto não é UTF-8 / text is not UTF-8")]
    InvalidUtf8,
    /// PT: Só categoria, linha e coluna: a mensagem do serde pode conter texto do usuário.
    /// EN: Only category, line and column: the serde message may contain user text.
    #[error("JSON inválido / invalid JSON ({category}, {line}:{column})")]
    InvalidJson {
        category: &'static str,
        line: usize,
        column: usize,
    },
    #[error("não é blueprint nem livro / not a blueprint or book")]
    NotABlueprint,
    #[error("limite excedido / limit exceeded: {what} ({found} > {max})")]
    LimitExceeded {
        what: &'static str,
        found: usize,
        max: usize,
    },
    #[error("livro aninhado demais / book nested too deeply (> {max})")]
    BookTooDeep { max: usize },
    #[error(
        "posição inválida na entidade {entity_number} / invalid position on entity {entity_number}"
    )]
    InvalidPosition { entity_number: u64 },
    #[error("blueprint grande demais em área / blueprint too large in area")]
    AreaTooLarge,
    #[error("número de entidade repetido / duplicate entity number {0}")]
    DuplicateEntityNumber(u64),
    #[error("número de entidade inválido / invalid entity number {0}")]
    InvalidEntityNumber(u64),
    #[error(
        "direção inválida na entidade {entity_number} / invalid direction on entity {entity_number}"
    )]
    InvalidDirection { entity_number: u64 },
    #[error("fio aponta para entidade inexistente / wire points to a missing entity {0}")]
    DanglingWire(u64),
    #[error("fio com conector inválido / wire with an invalid connector")]
    InvalidWire,
    #[error("texto longo demais / text too long: {what}")]
    TextTooLong { what: &'static str },
}

impl DecodeError {
    /// PT: Código estável do erro, usado como chave de tradução.
    /// EN: Stable error code, used as the translation key.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "blueprint.empty",
            Self::TooLarge { .. } => "blueprint.too_large",
            Self::UnsupportedVersion(_) => "blueprint.unsupported_version",
            Self::InvalidCharacter(_) => "blueprint.invalid_character",
            Self::InvalidBase64 => "blueprint.invalid_base64",
            Self::InvalidCompression => "blueprint.invalid_compression",
            Self::DecompressedTooLarge { .. } => "blueprint.decompressed_too_large",
            Self::CompressionRatio => "blueprint.compression_ratio",
            Self::InvalidUtf8 => "blueprint.invalid_utf8",
            Self::InvalidJson { .. } => "blueprint.invalid_json",
            Self::NotABlueprint => "blueprint.not_a_blueprint",
            Self::LimitExceeded { .. } => "blueprint.limit_exceeded",
            Self::BookTooDeep { .. } => "blueprint.book_too_deep",
            Self::InvalidPosition { .. } => "blueprint.invalid_position",
            Self::AreaTooLarge => "blueprint.area_too_large",
            Self::DuplicateEntityNumber(_) => "blueprint.duplicate_entity_number",
            Self::InvalidEntityNumber(_) => "blueprint.invalid_entity_number",
            Self::InvalidDirection { .. } => "blueprint.invalid_direction",
            Self::DanglingWire(_) => "blueprint.dangling_wire",
            Self::InvalidWire => "blueprint.invalid_wire",
            Self::TextTooLong { .. } => "blueprint.text_too_long",
        }
    }
}

/// PT: Falha ao gerar a string (só acontece com dados internos inconsistentes).
/// EN: Failure while producing the string (only happens with inconsistent internal data).
#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("falha ao serializar / serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("falha ao comprimir / compression failed: {0}")]
    Compression(#[from] std::io::Error),
}
