//! PT: Limites aplicados a toda blueprint lida. Os tetos `HARD_*` são verificados durante a
//!     própria desserialização e não podem ser aumentados em tempo de execução; `Limits` pode
//!     ser mais restrito (o servidor usa valores iguais ou menores que o navegador).
//! EN: Limits applied to every blueprint that is read. The `HARD_*` caps are checked during
//!     deserialization itself and cannot be raised at runtime; `Limits` may be stricter
//!     (the server uses values equal to or lower than the browser).

/// PT: Teto fixo de entidades por blueprint. EN: Fixed cap on entities per blueprint.
pub const HARD_MAX_ENTITIES: usize = 100_000;
/// PT: Teto fixo de tiles por blueprint. EN: Fixed cap on tiles per blueprint.
pub const HARD_MAX_TILES: usize = 200_000;
/// PT: Teto fixo de fios por blueprint. EN: Fixed cap on wires per blueprint.
pub const HARD_MAX_WIRES: usize = 200_000;
/// PT: Teto fixo de entradas por livro. EN: Fixed cap on entries per book.
pub const HARD_MAX_BOOK_ENTRIES: usize = 1_000;
/// PT: Teto fixo de ícones por blueprint (o jogo usa até 4). EN: Fixed cap on icons per blueprint (the game uses up to 4).
pub const HARD_MAX_ICONS: usize = 8;
/// PT: Teto fixo de itens pedidos por entidade (módulos, combustível...). EN: Fixed cap on item requests per entity (modules, fuel...).
pub const HARD_MAX_ITEM_REQUESTS: usize = 64;
/// PT: Profundidade máxima do JSON inteiro, conferida antes da leitura. Uma blueprint real fica
///     abaixo de 50 (8 níveis de livro × 3 + o conteúdo da entidade); o teto evita estouro de pilha
///     na leitura recursiva, inclusive no WASM, que tem pilha menor.
/// EN: Maximum depth of the whole JSON, checked before reading. A real blueprint stays below 50
///     (8 book levels × 3 + the entity contents); the cap prevents stack overflow in the recursive
///     read, including in WASM, which has a smaller stack.
pub const HARD_MAX_JSON_DEPTH: usize = 64;
/// PT: Teto fixo de posições de inventário por pedido. EN: Fixed cap on inventory positions per request.
pub const HARD_MAX_INVENTORY_POSITIONS: usize = 1_024;

/// PT: Limites configuráveis da leitura. Os padrões valem para o navegador e o servidor.
/// EN: Configurable reading limits. The defaults apply to the browser and the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// PT: Tamanho máximo do texto colado, em bytes. EN: Maximum size of the pasted text, in bytes.
    pub max_encoded_len: usize,
    /// PT: Tamanho máximo depois de descomprimir, em bytes. EN: Maximum size after decompression, in bytes.
    pub max_decoded_len: usize,
    /// PT: Razão máxima entre descomprimido e comprimido. EN: Maximum ratio between decompressed and compressed.
    pub max_compression_ratio: usize,
    pub max_entities: usize,
    pub max_tiles: usize,
    pub max_wires: usize,
    /// PT: Soma das entidades de todas as blueprints de um livro. EN: Sum of entities across all blueprints in a book.
    pub max_total_entities: usize,
    pub max_book_depth: usize,
    pub max_book_entries: usize,
    /// PT: Total de blueprints dentro de um livro, contando sub-livros. EN: Total blueprints inside a book, counting sub-books.
    pub max_total_blueprints: usize,
    /// PT: Coordenada máxima em valor absoluto, em tiles. EN: Maximum absolute coordinate, in tiles.
    pub max_coordinate: f64,
    /// PT: Área máxima da caixa envolvente de uma blueprint, em tiles². EN: Maximum bounding-box area of one blueprint, in tiles².
    pub max_bounding_area: f64,
    pub max_label_chars: usize,
    pub max_description_chars: usize,
    pub max_name_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_encoded_len: 8 * 1024 * 1024,
            max_decoded_len: 32 * 1024 * 1024,
            max_compression_ratio: 200,
            max_entities: HARD_MAX_ENTITIES,
            max_tiles: HARD_MAX_TILES,
            max_wires: HARD_MAX_WIRES,
            max_total_entities: 200_000,
            max_book_depth: 8,
            max_book_entries: HARD_MAX_BOOK_ENTRIES,
            max_total_blueprints: 2_000,
            max_coordinate: 100_000.0,
            max_bounding_area: 10_000.0 * 10_000.0,
            max_label_chars: 500,
            max_description_chars: 5_000,
            max_name_chars: 100,
        }
    }
}
