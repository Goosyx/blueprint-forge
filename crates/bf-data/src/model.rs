//! PT: Tipos normalizados dos dados do jogo, no formato do arquivo `gamedata.json`.
//!     Todas as energias estão em joules (J), as potências em watts (W) e os tempos em segundos.
//! EN: Normalized game data types, in the format of the `gamedata.json` file.
//!     All energies are in joules (J), powers in watts (W) and times in seconds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// PT: Versão do Factorio que o projeto usa. Os scripts leem a mesma versão de
///     `scripts/factorio-version.sh`, e um teste confere que as duas batem.
/// EN: Factorio version the project uses. The scripts read the same version from
///     `scripts/factorio-version.sh`, and a test checks that both match.
pub const GAME_VERSION: &str = "2.1.23";

/// PT: Mods oficiais que vêm com o jogo. Nas exportações e nos testes só o `base` fica ligado.
/// EN: Official mods shipped with the game. In exports and tests only `base` stays enabled.
pub const OFFICIAL_MODS: &[&str] = &["base", "elevated-rails", "quality", "recycler", "space-age"];

/// PT: Teto de produtividade padrão do jogo, usado quando a receita não define outro (ou não há
///     receita).
/// EN: The game's default productivity cap, used when the recipe sets no other (or there is no
///     recipe).
pub const DEFAULT_MAXIMUM_PRODUCTIVITY: f64 = 3.0;

/// PT: Versão do formato do `gamedata.json`. Muda sempre que um campo muda de sentido.
/// EN: Version of the `gamedata.json` format. It changes whenever a field changes meaning.
pub const FORMAT_VERSION: u32 = 2;

/// PT: Todos os dados de uma versão do jogo que o projeto usa.
/// EN: All the data of one game version that the project uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameData {
    pub format_version: u32,
    /// PT: Versão do jogo de onde os dados vieram, por exemplo `"2.1.23"`.
    /// EN: Game version the data came from, for example `"2.1.23"`.
    pub game_version: String,
    pub items: BTreeMap<String, Item>,
    pub fluids: BTreeMap<String, Fluid>,
    pub recipes: BTreeMap<String, Recipe>,
    pub entities: BTreeMap<String, Entity>,
    /// PT: Tiles que podem ser colocados por item (concreto, piso de pedra...).
    /// EN: Tiles that can be placed by an item (concrete, stone path...).
    pub tiles: BTreeMap<String, Tile>,
}

/// PT: Item do jogo (inclui módulos, frascos de ciência e itens que constroem entidades).
/// EN: Game item (includes modules, science packs and items that build entities).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    /// PT: Tipo do protótipo no jogo (`item`, `module`, `rail-planner`...).
    /// EN: Prototype type in the game (`item`, `module`, `rail-planner`...).
    pub prototype: String,
    pub stack_size: u32,
    /// PT: Entidades que este item constrói (`place_result` ou os trilhos de um `rail-planner`).
    /// EN: Entities this item builds (`place_result` or a `rail-planner`'s rails).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub places_entities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub places_tile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<Fuel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<Module>,
    #[serde(default)]
    pub subgroup: String,
    #[serde(default)]
    pub order: String,
}

/// PT: Dados de combustível de um item.
/// EN: Fuel data of an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fuel {
    /// PT: Categorias de combustível do item (a 2.1 permite mais de uma).
    /// EN: The item's fuel categories (2.1 allows more than one).
    pub categories: Vec<String>,
    pub value_j: f64,
}

/// PT: Dados de um módulo: categoria (velocidade, produtividade...), nível e efeito.
/// EN: Module data: category (speed, productivity...), tier and effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub category: String,
    pub tier: u32,
    pub effect: Effect,
}

/// PT: Efeito somável de módulos e beacons. Os valores são frações: `0.5` é +50%.
/// EN: Additive effect of modules and beacons. Values are fractions: `0.5` means +50%.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub productivity: f64,
    #[serde(default)]
    pub consumption: f64,
    #[serde(default)]
    pub pollution: f64,
    #[serde(default)]
    pub quality: f64,
}

/// PT: Fluido do jogo.
/// EN: Game fluid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fluid {
    pub name: String,
    pub default_temperature: f64,
    pub max_temperature: f64,
    /// PT: Energia para aquecer uma unidade em 1 °C.
    /// EN: Energy to heat one unit by 1 °C.
    pub heat_capacity_j: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_value_j: Option<f64>,
    #[serde(default)]
    pub subgroup: String,
    #[serde(default)]
    pub order: String,
}

/// PT: Tile que pode ser construído por um item.
/// EN: Tile that can be built by an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placed_by: Vec<String>,
}

/// PT: Se um ingrediente ou produto é item ou fluido.
/// EN: Whether an ingredient or product is an item or a fluid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Item,
    Fluid,
}

/// PT: Receita do jogo.
/// EN: Game recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    /// PT: Categorias da receita; uma máquina pode fazê-la se tiver qualquer uma delas (2.1+).
    /// EN: Recipe categories; a machine can craft it if it has any of them (2.1+).
    pub categories: Vec<String>,
    /// PT: Tempo da receita em segundos, com máquina de velocidade 1.
    /// EN: Recipe time in seconds, with a speed-1 machine.
    pub energy_required: f64,
    pub ingredients: Vec<Ingredient>,
    pub results: Vec<Product>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_product: Option<String>,
    pub allow_productivity: bool,
    pub maximum_productivity: f64,
    pub emissions_multiplier: f64,
    /// PT: Se a receita já vem liberada sem pesquisa.
    /// EN: Whether the recipe is available without research.
    pub enabled: bool,
    pub hidden: bool,
    #[serde(default)]
    pub subgroup: String,
    #[serde(default)]
    pub order: String,
}

/// PT: Ingrediente de uma receita.
/// EN: Recipe ingredient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ingredient {
    pub kind: ResourceKind,
    pub name: String,
    pub amount: f64,
}

/// PT: Produto de uma receita. A quantidade média por ciclo sai de `expected_amount`.
/// EN: Recipe product. The average amount per cycle comes from `expected_amount`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub kind: ResourceKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_max: Option<f64>,
    /// PT: Chance de sair em cada ciclo (independente × faixa compartilhada, na 2.1+).
    /// EN: Chance of coming out on each cycle (independent × shared range, in 2.1+).
    pub probability: f64,
    pub extra_count_fraction: f64,
    /// PT: Parte do produto que não recebe bônus de produtividade.
    /// EN: Part of the product that does not receive the productivity bonus.
    pub ignored_by_productivity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

impl Product {
    /// PT: Quantidade média produzida por ciclo, sem produtividade.
    /// EN: Average amount produced per cycle, without productivity.
    pub fn expected_amount(&self) -> f64 {
        let base = match (self.amount, self.amount_min, self.amount_max) {
            (Some(amount), _, _) => amount,
            (None, Some(min), Some(max)) => (min + max) / 2.0,
            _ => 0.0,
        };
        base * self.probability + self.extra_count_fraction
    }
}

/// PT: Tamanho de uma entidade em tiles, virada para o norte.
/// EN: Size of an entity in tiles, facing north.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileSize {
    pub width: u32,
    pub height: u32,
}

/// PT: Entidade construível do jogo.
/// EN: Buildable game entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    /// PT: Tipo do protótipo no jogo (`assembling-machine`, `inserter`...).
    /// EN: Prototype type in the game (`assembling-machine`, `inserter`...).
    pub prototype: String,
    pub size: TileSize,
    /// PT: Caixa de colisão relativa ao centro: `[[x1, y1], [x2, y2]]`.
    /// EN: Collision box relative to the center: `[[x1, y1], [x2, y2]]`.
    pub collision_box: [[f64; 2]; 2],
    /// PT: Itens que constroem esta entidade.
    /// EN: Items that build this entity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placed_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_source: Option<EnergySource>,
    /// PT: Consumo nominal em funcionamento, quando a entidade tem um.
    /// EN: Nominal consumption while working, when the entity has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_usage_w: Option<f64>,
    pub kind: EntityKind,
}

/// PT: Fonte de energia de uma entidade.
/// EN: Energy source of an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergySource {
    pub kind: EnergySourceKind,
    /// PT: Consumo parado. Nas máquinas elétricas o jogo usa `energy_usage / 30` quando não vem no dado.
    /// EN: Idle consumption. For electric machines the game uses `energy_usage / 30` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain_w: Option<f64>,
    /// PT: Emissões por minuto em funcionamento, por poluente (no base só `pollution`).
    /// EN: Emissions per minute while working, per pollutant (only `pollution` in base).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub emissions_per_minute: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fuel_categories: Vec<String>,
    pub effectivity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_capacity_j: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_flow_limit_w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_flow_limit_w: Option<f64>,
}

/// PT: Tipo de fonte de energia.
/// EN: Energy source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnergySourceKind {
    Electric,
    Burner,
    Heat,
    Fluid,
    Void,
}

/// PT: Conexão de cano de uma caixa de fluido, relativa ao centro com a entidade virada para o norte.
/// EN: Pipe connection of a fluid box, relative to the center with the entity facing north.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipeConnection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f64; 2]>,
    /// PT: Direção da conexão em 16 direções (0 = norte, 4 = leste...).
    /// EN: Connection direction in 16 directions (0 = north, 4 = east...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<u8>,
    pub flow_direction: String,
    pub connection_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_underground_distance: Option<u32>,
}

/// PT: Caixa de fluido de uma entidade.
/// EN: Fluid box of an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluidBox {
    /// PT: `input`, `output`, `input-output` ou `none`.
    /// EN: `input`, `output`, `input-output` or `none`.
    pub production_type: String,
    pub volume: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    pub pipe_connections: Vec<PipeConnection>,
}

/// PT: Dados específicos de cada tipo de entidade que o projeto usa.
/// EN: Type-specific data of each entity kind the project uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EntityKind {
    CraftingMachine {
        crafting_speed: f64,
        crafting_categories: Vec<String>,
        module_slots: u32,
        allowed_effects: Vec<String>,
        uses_module_effects: bool,
        uses_beacon_effects: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fixed_recipe: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fluid_boxes: Vec<FluidBox>,
    },
    Beacon {
        supply_area_distance: f64,
        distribution_effectivity: f64,
        profile: Vec<f64>,
        module_slots: u32,
        allowed_effects: Vec<String>,
    },
    TransportBelt {
        /// PT: Velocidade em tiles por tick; a vazão da esteira é `speed × 480` itens/s.
        /// EN: Speed in tiles per tick; the belt throughput is `speed × 480` items/s.
        speed: f64,
    },
    UndergroundBelt {
        speed: f64,
        max_distance: u32,
    },
    Splitter {
        speed: f64,
    },
    Loader {
        speed: f64,
    },
    Inserter {
        pickup_position: [f64; 2],
        insert_position: [f64; 2],
        rotation_speed: f64,
        extension_speed: f64,
        energy_per_movement_j: f64,
        energy_per_rotation_j: f64,
        bulk: bool,
    },
    ElectricPole {
        supply_area_distance: f64,
        maximum_wire_distance: f64,
    },
    Container {
        inventory_size: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logistic_mode: Option<String>,
    },
    Boiler {
        target_temperature: f64,
        energy_consumption_w: f64,
    },
    Generator {
        fluid_usage_per_tick: f64,
        maximum_temperature: f64,
        effectivity: f64,
    },
    SolarPanel {
        production_w: f64,
    },
    Accumulator,
    Reactor {
        consumption_w: f64,
        neighbour_bonus: f64,
    },
    MiningDrill {
        mining_speed: f64,
        resource_categories: Vec<String>,
        module_slots: u32,
    },
    Lab {
        researching_speed: f64,
        module_slots: u32,
        inputs: Vec<String>,
    },
    Pipe,
    PipeToGround {
        max_underground_distance: u32,
    },
    Pump {
        pumping_speed: f64,
    },
    OffshorePump {
        pumping_speed: f64,
    },
    StorageTank,
    ConstantCombinator,
    /// PT: Qualquer outra entidade construível (muros, lâmpadas, trilhos...).
    /// EN: Any other buildable entity (walls, lamps, rails...).
    Other,
}

impl GameData {
    /// PT: Lê o `gamedata.json` e confere a versão do formato.
    /// EN: Reads `gamedata.json` and checks the format version.
    pub fn from_json(text: &str) -> Result<Self, LoadError> {
        // PT: A versão é lida antes do resto: um pacote de outro formato dá erro de versão, não
        //     um erro de campo ausente.
        // EN: The version is read before the rest: a package in another format gives a version
        //     error, not a missing-field error.
        #[derive(Deserialize)]
        struct Header {
            format_version: u32,
        }
        let header: Header = serde_json::from_str(text)?;
        if header.format_version != FORMAT_VERSION {
            return Err(LoadError::FormatVersion {
                found: header.format_version,
                expected: FORMAT_VERSION,
            });
        }
        let data: GameData = serde_json::from_str(text)?;
        if data.format_version != FORMAT_VERSION {
            return Err(LoadError::FormatVersion {
                found: data.format_version,
                expected: FORMAT_VERSION,
            });
        }
        Ok(data)
    }

    /// PT: Indica se o nome é de um fluido (senão é tratado como item).
    /// EN: Tells whether the name is a fluid (otherwise it is treated as an item).
    pub fn is_fluid(&self, name: &str) -> bool {
        self.fluids.contains_key(name)
    }
}

/// PT: Erro ao carregar o `gamedata.json`.
/// EN: Error while loading `gamedata.json`.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("JSON inválido / invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "versão do formato {found}, esperada {expected} / format version {found}, expected {expected}"
    )]
    FormatVersion { found: u32, expected: u32 },
}
