//! PT: Modelo do JSON de blueprint do Factorio 2.0. As listas grandes têm teto fixo na
//!     desserialização (`bounded_vec`). Campos que o projeto não interpreta ficam como JSON cru
//!     (`RawJson`): uma cópia do texto, sem montar árvores na memória. Nada usa `flatten`, que
//!     copiaria a entrada para um buffer genérico antes de qualquer limite.
//! EN: Model of the Factorio 2.0 blueprint JSON. Large lists have a fixed cap during
//!     deserialization (`bounded_vec`). Fields the project does not interpret stay as raw JSON
//!     (`RawJson`): a copy of the text, without building trees in memory. Nothing uses `flatten`,
//!     which would copy the input into a generic buffer before any limit.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, DeserializeOwned, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::bounded::{bounded_vec, limit_error};
use crate::limits::{
    HARD_MAX_BOOK_ENTRIES, HARD_MAX_ENTITIES, HARD_MAX_ICONS, HARD_MAX_INVENTORY_POSITIONS,
    HARD_MAX_ITEM_REQUESTS, HARD_MAX_TILES, HARD_MAX_WIRES,
};

/// PT: Teto de campos não interpretados por entidade.
/// EN: Cap on uninterpreted fields per entity.
pub const HARD_MAX_EXTRA_FIELDS: usize = 32;

/// PT: JSON guardado como texto, sem ser interpretado. A memória usada é o tamanho do texto.
/// EN: JSON kept as text, without being interpreted. Memory used is the size of the text.
#[derive(Debug, Clone)]
pub struct RawJson(Box<RawValue>);

impl RawJson {
    /// PT: Texto JSON original. EN: Original JSON text.
    pub fn get(&self) -> &str {
        self.0.get()
    }

    /// PT: Cria a partir de um valor serializável. EN: Creates it from a serializable value.
    pub fn from_value<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        serde_json::value::to_raw_value(value).map(Self)
    }

    /// PT: Interpreta o texto como `T`. EN: Parses the text as `T`.
    pub fn parse<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(self.0.get())
    }
}

impl PartialEq for RawJson {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Serialize for RawJson {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// PT: Profundidade máxima aceita dentro de um JSON cru (a mesma do `serde_json`).
/// EN: Maximum depth accepted inside raw JSON (the same as `serde_json`).
pub const MAX_RAW_DEPTH: usize = 128;

impl<'de> Deserialize<'de> for RawJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        if json_depth(raw.get()) > MAX_RAW_DEPTH {
            return Err(de::Error::custom("nesting too deep"));
        }
        Ok(Self(raw))
    }
}

// PT: Profundidade de aninhamento de um texto JSON já validado, numa passada linear e sem
//     recursão (colchetes dentro de strings não contam).
// EN: Nesting depth of already-validated JSON text, in one linear pass without recursion
//     (brackets inside strings do not count).
pub(crate) fn json_depth(text: &str) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// PT: Versão gravada nas blueprints geradas: 2.1.23.
/// EN: Version written into generated blueprints: 2.1.23.
pub const GAME_VERSION: u64 = pack_version(2, 1, 23, 0);

/// PT: Empacota a versão no formato do jogo: 16 bits para cada parte.
/// EN: Packs the version in the game's format: 16 bits for each part.
pub const fn pack_version(major: u16, minor: u16, patch: u16, build: u16) -> u64 {
    ((major as u64) << 48) | ((minor as u64) << 32) | ((patch as u64) << 16) | (build as u64)
}

/// PT: Parte "maior" da versão empacotada (1 para 1.1, 2 para 2.0).
/// EN: "Major" part of the packed version (1 for 1.1, 2 for 2.0).
pub const fn major_version(version: u64) -> u64 {
    version >> 48
}

/// PT: Conteúdo de uma blueprint string: uma blueprint ou um livro.
/// EN: Contents of a blueprint string: a blueprint or a book.
#[derive(Debug, Clone, PartialEq)]
pub enum BlueprintString {
    Blueprint(Blueprint),
    Book(BlueprintBook),
}

// PT: Forma do JSON na raiz; convertida para `BlueprintString` pelo codec.
// EN: Shape of the root JSON; converted into `BlueprintString` by the codec.
#[derive(Debug, Deserialize, Serialize, Default)]
pub(crate) struct Root {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<Blueprint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_book: Option<BlueprintBook>,
}

/// PT: Posição em tiles. Na blueprint é o centro da entidade.
/// EN: Position in tiles. In a blueprint it is the entity's center.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// PT: Sinal usado em ícones (item, fluido, sinal virtual...).
/// EN: Signal used in icons (item, fluid, virtual signal...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
}

/// PT: Ícone da blueprint (até 4 no jogo).
/// EN: Blueprint icon (up to 4 in the game).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub signal: Signal,
    pub index: u32,
}

/// PT: Uma blueprint.
/// EN: One blueprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    #[serde(default = "default_blueprint_item")]
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<RawJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_ICONS>"
    )]
    pub icons: Vec<Icon>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_ENTITIES>"
    )]
    pub entities: Vec<Entity>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_TILES>"
    )]
    pub tiles: Vec<TileEntry>,
    /// PT: Fios do 2.0: `[entidade_a, conector_a, entidade_b, conector_b]` (5 = cobre de poste).
    /// EN: 2.0 wires: `[entity_a, connector_a, entity_b, connector_b]` (5 = pole copper).
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_WIRES>"
    )]
    pub wires: Vec<[u64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedules: Option<RawJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_connections: Option<RawJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<RawJson>,
    #[serde(
        rename = "snap-to-grid",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub snap_to_grid: Option<Position>,
    #[serde(
        rename = "absolute-snapping",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub absolute_snapping: Option<bool>,
    #[serde(
        rename = "position-relative-to-grid",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub position_relative_to_grid: Option<Position>,
    #[serde(default)]
    pub version: u64,
}

fn default_blueprint_item() -> String {
    "blueprint".to_owned()
}

impl Default for Blueprint {
    fn default() -> Self {
        Self {
            item: default_blueprint_item(),
            label: None,
            label_color: None,
            description: None,
            icons: Vec::new(),
            entities: Vec::new(),
            tiles: Vec::new(),
            wires: Vec::new(),
            schedules: None,
            stock_connections: None,
            parameters: None,
            snap_to_grid: None,
            absolute_snapping: None,
            position_relative_to_grid: None,
            version: GAME_VERSION,
        }
    }
}

/// PT: Tile colocado pela blueprint (concreto, piso...).
/// EN: Tile placed by the blueprint (concrete, path...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileEntry {
    pub name: String,
    pub position: Position,
}

/// PT: Uma entidade da blueprint. Campos que o projeto não interpreta ficam em `extra`
///     (filtros, comportamento de circuito...) e voltam iguais na gravação.
/// EN: One blueprint entity. Fields the project does not interpret stay in `extra`
///     (filters, circuit behavior...) and are written back unchanged.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entity {
    pub entity_number: u64,
    pub name: String,
    pub position: Position,
    /// PT: Direção em 16 passos (0 norte, 4 leste, 8 sul, 12 oeste). O codec converte o 1.1.
    /// EN: Direction in 16 steps (0 north, 4 east, 8 south, 12 west). The codec converts 1.1.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub direction: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_quality: Option<String>,
    /// PT: Tipo de underground ou loader: `"input"` ou `"output"`.
    /// EN: Underground or loader type: `"input"` or `"output"`.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub io_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<ItemRequests>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, RawJson>,
}

// PT: Leitura manual da entidade: campos conhecidos vão direto para os tipos, desconhecidos viram
//     `RawJson` (com teto de quantidade), e nada passa por buffer genérico.
// EN: Manual entity reading: known fields go straight into their types, unknown ones become
//     `RawJson` (with a count cap), and nothing goes through a generic buffer.
impl<'de> Deserialize<'de> for Entity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntityVisitor;

        impl<'de> Visitor<'de> for EntityVisitor {
            type Value = Entity;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("uma entidade / an entity")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entity, A::Error> {
                fn set<T, E: de::Error>(
                    slot: &mut Option<T>,
                    value: T,
                    field: &'static str,
                ) -> Result<(), E> {
                    if slot.is_some() {
                        return Err(E::duplicate_field(field));
                    }
                    *slot = Some(value);
                    Ok(())
                }

                let mut entity_number = None;
                let mut name = None;
                let mut position = None;
                let mut direction = None;
                let mut mirror = None;
                let mut quality = None;
                let mut recipe = None;
                let mut recipe_quality = None;
                let mut io_type = None;
                let mut items = None;
                let mut extra = BTreeMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "entity_number" => {
                            set(&mut entity_number, map.next_value()?, "entity_number")?
                        }
                        "name" => set(&mut name, map.next_value()?, "name")?,
                        "position" => set(&mut position, map.next_value()?, "position")?,
                        "direction" => set(&mut direction, map.next_value()?, "direction")?,
                        "mirror" => set(&mut mirror, map.next_value()?, "mirror")?,
                        "quality" => set(&mut quality, map.next_value()?, "quality")?,
                        "recipe" => set(&mut recipe, map.next_value()?, "recipe")?,
                        "recipe_quality" => {
                            set(&mut recipe_quality, map.next_value()?, "recipe_quality")?
                        }
                        "type" => set(&mut io_type, map.next_value()?, "type")?,
                        "items" => set(&mut items, map.next_value()?, "items")?,
                        _ => {
                            if extra.len() >= HARD_MAX_EXTRA_FIELDS {
                                return Err(limit_error(HARD_MAX_EXTRA_FIELDS));
                            }
                            if extra.contains_key(&key) {
                                return Err(de::Error::custom("duplicate field"));
                            }
                            let value: RawJson = map.next_value()?;
                            extra.insert(key, value);
                        }
                    }
                }

                Ok(Entity {
                    entity_number: entity_number
                        .ok_or_else(|| de::Error::missing_field("entity_number"))?,
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    position: position.ok_or_else(|| de::Error::missing_field("position"))?,
                    direction: direction.unwrap_or(0),
                    mirror,
                    quality,
                    recipe,
                    recipe_quality,
                    io_type,
                    items,
                    extra,
                })
            }
        }

        deserializer.deserialize_map(EntityVisitor)
    }
}

impl Entity {
    /// PT: Entidade mínima, com os campos opcionais vazios.
    /// EN: Minimal entity, with the optional fields empty.
    pub fn new(entity_number: u64, name: impl Into<String>, position: Position) -> Self {
        Self {
            entity_number,
            name: name.into(),
            position,
            direction: 0,
            mirror: None,
            quality: None,
            recipe: None,
            recipe_quality: None,
            io_type: None,
            items: None,
            extra: BTreeMap::new(),
        }
    }
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

/// PT: Itens pedidos para dentro de uma entidade (módulos, combustível...).
///     O 2.0 usa uma lista com as posições de inventário; o 1.1 usava `{nome: quantidade}`.
/// EN: Items requested into an entity (modules, fuel...).
///     2.0 uses a list with inventory positions; 1.1 used `{name: count}`.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemRequests {
    Modern(Vec<ItemRequest>),
    Legacy(BTreeMap<String, u32>),
}

impl ItemRequests {
    /// PT: Quantidade total pedida de cada item.
    /// EN: Total requested count of each item.
    pub fn counts(&self) -> BTreeMap<String, u32> {
        let mut counts = BTreeMap::new();
        match self {
            Self::Legacy(map) => {
                for (name, count) in map {
                    *counts.entry(name.clone()).or_insert(0) += count;
                }
            }
            Self::Modern(requests) => {
                for request in requests {
                    let total: u32 = request
                        .items
                        .in_inventory
                        .iter()
                        .map(|position| position.count.unwrap_or(1))
                        .fold(0u32, u32::saturating_add)
                        .saturating_add(request.items.grid_count.unwrap_or(0));
                    let entry = counts.entry(request.id.name.clone()).or_insert(0u32);
                    *entry = entry.saturating_add(total);
                }
            }
        }
        counts
    }
}

/// PT: Pedido de um item no formato 2.0.
/// EN: Item request in the 2.0 format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemRequest {
    pub id: ItemId,
    pub items: ItemPlacement,
}

/// PT: Identificação do item pedido.
/// EN: Requested item identification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemId {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
}

/// PT: Onde os itens pedidos vão dentro da entidade.
/// EN: Where the requested items go inside the entity.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ItemPlacement {
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_INVENTORY_POSITIONS>"
    )]
    pub in_inventory: Vec<InventoryPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_count: Option<u32>,
}

/// PT: Posição de inventário de um item pedido.
/// EN: Inventory position of a requested item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryPosition {
    pub inventory: u32,
    pub stack: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

impl Serialize for ItemRequests {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Modern(requests) => requests.serialize(serializer),
            Self::Legacy(map) => map.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ItemRequests {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RequestsVisitor;

        impl<'de> Visitor<'de> for RequestsVisitor {
            type Value = ItemRequests;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "lista de pedidos ou mapa nome→quantidade / request list or name→count map",
                )
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut requests = Vec::new();
                while let Some(request) = seq.next_element::<ItemRequest>()? {
                    if requests.len() >= HARD_MAX_ITEM_REQUESTS {
                        return Err(limit_error(HARD_MAX_ITEM_REQUESTS));
                    }
                    requests.push(request);
                }
                Ok(ItemRequests::Modern(requests))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut counts = BTreeMap::new();
                while let Some((name, count)) = map.next_entry::<String, u32>()? {
                    if counts.len() >= HARD_MAX_ITEM_REQUESTS {
                        return Err(limit_error(HARD_MAX_ITEM_REQUESTS));
                    }
                    counts.insert(name, count);
                }
                Ok(ItemRequests::Legacy(counts))
            }
        }

        deserializer.deserialize_any(RequestsVisitor)
    }
}

/// PT: Livro de blueprints. Pode conter outros livros.
/// EN: Blueprint book. It may contain other books.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintBook {
    #[serde(default = "default_book_item")]
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<RawJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_ICONS>"
    )]
    pub icons: Vec<Icon>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "bounded_vec::<_, _, HARD_MAX_BOOK_ENTRIES>"
    )]
    pub blueprints: Vec<BookEntry>,
    #[serde(default)]
    pub active_index: u32,
    #[serde(default)]
    pub version: u64,
}

fn default_book_item() -> String {
    "blueprint-book".to_owned()
}

/// PT: Entrada de um livro. Planejadores de upgrade e desconstrução são mantidos sem interpretação.
/// EN: Book entry. Upgrade and deconstruction planners are kept without interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookEntry {
    #[serde(default)]
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<Blueprint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_book: Option<Box<BlueprintBook>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade_planner: Option<RawJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deconstruction_planner: Option<RawJson>,
}
