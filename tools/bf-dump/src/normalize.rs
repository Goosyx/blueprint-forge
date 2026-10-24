//! PT: Converte o `data-raw-dump.json` do jogo (Lua serializado em JSON) nos tipos
//!     normalizados do `bf-data`. Os padrões implícitos do jogo são aplicados aqui.
//! EN: Converts the game's `data-raw-dump.json` (Lua serialized as JSON) into the normalized
//!     `bf-data` types. The game's implicit defaults are applied here.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};
use bf_data::units::{Quantity, parse_si};
use bf_data::{
    EnergySource, EnergySourceKind, Entity, EntityKind, Fluid, FluidBox, Fuel, GameData,
    Ingredient, Item, Module, PipeConnection, Product, Recipe, ResourceKind, Tile, TileSize,
};
use serde_json::{Map, Value};

/// PT: Tipos de protótipo que são itens (todos têm `stack_size`).
/// EN: Prototype types that are items (all of them have `stack_size`).
const ITEM_TYPES: &[&str] = &[
    "item",
    "ammo",
    "armor",
    "blueprint",
    "blueprint-book",
    "capsule",
    "copy-paste-tool",
    "deconstruction-item",
    "gun",
    "item-with-entity-data",
    "item-with-inventory",
    "item-with-label",
    "item-with-tags",
    "module",
    "rail-planner",
    "repair-tool",
    "selection-tool",
    "space-platform-starter-pack",
    "spidertron-remote",
    // PT: Tipo do 2.0 (frascos de ciência); a 2.1 os trocou por `item`.
    // EN: 2.0 type (science packs); 2.1 replaced them with `item`.
    "tool",
    "upgrade-item",
];

/// PT: Tipos de protótipo que não são entidades, ignorados na busca de entidades.
/// EN: Prototype types that are not entities, skipped when looking up entities.
const NON_ENTITY_TYPES: &[&str] = &["recipe", "fluid", "technology", "tile", "item-subgroup"];

type Object = Map<String, Value>;

/// PT: Converte o dump inteiro. `game_version` vai para o campo `game_version`.
/// EN: Converts the whole dump. `game_version` goes into the `game_version` field.
pub fn normalize(raw: &Value, game_version: &str) -> Result<GameData> {
    let raw = raw
        .as_object()
        .ok_or_else(|| anyhow!("o dump não é um objeto / the dump is not an object"))?;

    let mut items = BTreeMap::new();
    for item_type in ITEM_TYPES {
        for (name, proto) in prototypes(raw, item_type) {
            let item = item(name, item_type, proto).with_context(|| format!("item {name:?}"))?;
            items.insert(name.clone(), item);
        }
    }

    let mut fluids = BTreeMap::new();
    for (name, proto) in prototypes(raw, "fluid") {
        fluids.insert(
            name.clone(),
            fluid(name, proto).with_context(|| format!("fluid {name:?}"))?,
        );
    }

    let mut recipes = BTreeMap::new();
    for (name, proto) in prototypes(raw, "recipe") {
        recipes.insert(
            name.clone(),
            recipe(name, proto).with_context(|| format!("recipe {name:?}"))?,
        );
    }

    // PT: Entidades construíveis são as que algum item constrói.
    // EN: Buildable entities are the ones some item builds.
    let mut placed_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut tile_placed_by: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for item in items.values() {
        for entity in &item.places_entities {
            placed_by
                .entry(entity.clone())
                .or_default()
                .insert(item.name.clone());
        }
        if let Some(tile) = &item.places_tile {
            tile_placed_by
                .entry(tile.clone())
                .or_default()
                .insert(item.name.clone());
        }
    }

    let entity_index = entity_index(raw);
    let mut entities = BTreeMap::new();
    for (name, items_placing) in &placed_by {
        let Some((prototype, proto)) = entity_index.get(name.as_str()) else {
            continue;
        };
        let mut entity =
            entity(name, prototype, proto).with_context(|| format!("entity {name:?}"))?;
        entity.placed_by = items_placing.iter().cloned().collect();
        entities.insert(name.clone(), entity);
    }

    let tiles = tile_placed_by
        .into_iter()
        .map(|(name, placed)| {
            let tile = Tile {
                name: name.clone(),
                placed_by: placed.into_iter().collect(),
            };
            (name, tile)
        })
        .collect();

    Ok(GameData {
        format_version: bf_data::FORMAT_VERSION,
        game_version: game_version.to_owned(),
        items,
        fluids,
        recipes,
        entities,
        tiles,
    })
}

// PT: Protótipos de um tipo, em ordem de nome. Tipo ausente vira lista vazia.
// EN: Prototypes of one type, in name order. A missing type becomes an empty list.
fn prototypes<'a>(raw: &'a Object, prototype: &str) -> Vec<(&'a String, &'a Object)> {
    let Some(Value::Object(group)) = raw.get(prototype) else {
        return Vec::new();
    };
    group
        .iter()
        .filter_map(|(name, proto)| proto.as_object().map(|proto| (name, proto)))
        .collect()
}

// PT: Índice nome → (tipo, protótipo) de tudo que tem caixa de colisão.
// EN: Name → (type, prototype) index of everything that has a collision box.
fn entity_index(raw: &Object) -> BTreeMap<&str, (&str, &Object)> {
    let mut index = BTreeMap::new();
    for (prototype, group) in raw {
        if ITEM_TYPES.contains(&prototype.as_str())
            || NON_ENTITY_TYPES.contains(&prototype.as_str())
        {
            continue;
        }
        let Value::Object(group) = group else {
            continue;
        };
        for (name, proto) in group {
            if let Value::Object(proto) = proto
                && proto.contains_key("collision_box")
            {
                index.insert(name.as_str(), (prototype.as_str(), proto));
            }
        }
    }
    index
}

fn item(name: &str, prototype: &str, proto: &Object) -> Result<Item> {
    let mut places_entities = Vec::new();
    if let Some(result) = str_field(proto, "place_result").filter(|s| !s.is_empty()) {
        places_entities.push(result.to_owned());
    }
    // PT: No 2.0 o `rail-planner` lista os trilhos que constrói em `rails`.
    // EN: In 2.0 the `rail-planner` lists the rails it builds in `rails`.
    for rail in list(proto.get("rails")) {
        if let Some(rail) = rail.as_str() {
            places_entities.push(rail.to_owned());
        }
    }
    let places_tile = proto
        .get("place_as_tile")
        .and_then(Value::as_object)
        .and_then(|tile| str_field(tile, "result"))
        .map(str::to_owned);

    // PT: A 2.1 trocou `fuel_category` por `fuel_categories`; o formato antigo ainda é aceito.
    // EN: 2.1 replaced `fuel_category` with `fuel_categories`; the old format is still accepted.
    let mut fuel_categories: Vec<String> = list(proto.get("fuel_categories"))
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    if fuel_categories.is_empty()
        && let Some(category) = str_field(proto, "fuel_category")
    {
        fuel_categories.push(category.to_owned());
    }
    let fuel = match (fuel_categories.is_empty(), str_field(proto, "fuel_value")) {
        (false, Some(value)) => Some(Fuel {
            categories: fuel_categories,
            value_j: parse_si(value, Quantity::Energy)?,
        }),
        _ => None,
    };

    let module = if prototype == "module" {
        Some(Module {
            category: str_field(proto, "category").unwrap_or_default().to_owned(),
            tier: u32_field(proto, "tier").unwrap_or(1),
            effect: effect(proto.get("effect"))?,
        })
    } else {
        None
    };

    Ok(Item {
        name: name.to_owned(),
        prototype: prototype.to_owned(),
        stack_size: u32_field(proto, "stack_size").unwrap_or(1),
        places_entities,
        places_tile,
        fuel,
        module,
        subgroup: str_field(proto, "subgroup").unwrap_or_default().to_owned(),
        order: str_field(proto, "order").unwrap_or_default().to_owned(),
    })
}

// PT: A 2.1 trocou `category` por `categories`; sem nenhuma das duas o jogo usa `crafting`.
// EN: 2.1 replaced `category` with `categories`; with neither the game uses `crafting`.
fn recipe_categories(proto: &Object) -> Vec<String> {
    let mut categories: Vec<String> = list(proto.get("categories"))
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    if categories.is_empty() {
        categories.push(
            str_field(proto, "category")
                .unwrap_or("crafting")
                .to_owned(),
        );
    }
    categories
}

// PT: Na 2.1 a chance é `independent_probability` (antes `probability`) vezes a largura da faixa
//     `shared_probability`, sorteada uma vez por ciclo para todos os produtos da receita.
// EN: In 2.1 the chance is `independent_probability` (formerly `probability`) times the width of
//     the `shared_probability` range, rolled once per cycle for all of the recipe's products.
fn product_probability(entry: &Object) -> f64 {
    let independent = f64_field(entry, "independent_probability")
        .or_else(|| f64_field(entry, "probability"))
        .unwrap_or(1.0);
    let shared = entry
        .get("shared_probability")
        .and_then(Value::as_object)
        .map_or(1.0, |range| {
            let min = f64_field(range, "min").unwrap_or(0.0);
            let max = f64_field(range, "max").unwrap_or(1.0);
            // PT: Só a parte da faixa dentro de [0, 1) pode sair no sorteio.
            // EN: Only the part of the range inside [0, 1) can come out of the roll.
            (max.min(1.0) - min.max(0.0)).max(0.0)
        });
    (independent * shared).clamp(0.0, 1.0)
}

fn effect(value: Option<&Value>) -> Result<bf_data::Effect> {
    let Some(Value::Object(effect)) = value else {
        return Ok(bf_data::Effect::default());
    };
    Ok(bf_data::Effect {
        speed: f64_field(effect, "speed").unwrap_or(0.0),
        productivity: f64_field(effect, "productivity").unwrap_or(0.0),
        consumption: f64_field(effect, "consumption").unwrap_or(0.0),
        pollution: f64_field(effect, "pollution").unwrap_or(0.0),
        quality: f64_field(effect, "quality").unwrap_or(0.0),
    })
}

fn fluid(name: &str, proto: &Object) -> Result<Fluid> {
    let default_temperature = f64_field(proto, "default_temperature").unwrap_or(15.0);
    Ok(Fluid {
        name: name.to_owned(),
        default_temperature,
        max_temperature: f64_field(proto, "max_temperature").unwrap_or(default_temperature),
        heat_capacity_j: match str_field(proto, "heat_capacity") {
            Some(value) => parse_si(value, Quantity::Energy)?,
            None => 1_000.0,
        },
        fuel_value_j: match str_field(proto, "fuel_value") {
            Some(value) => Some(parse_si(value, Quantity::Energy)?),
            None => None,
        },
        subgroup: str_field(proto, "subgroup").unwrap_or_default().to_owned(),
        order: str_field(proto, "order").unwrap_or_default().to_owned(),
    })
}

fn recipe(name: &str, proto: &Object) -> Result<Recipe> {
    let mut ingredients = Vec::new();
    for entry in list(proto.get("ingredients")) {
        let entry = entry
            .as_object()
            .ok_or_else(|| anyhow!("ingrediente inválido / invalid ingredient"))?;
        ingredients.push(Ingredient {
            kind: resource_kind(entry)?,
            name: required_str(entry, "name")?.to_owned(),
            amount: f64_field(entry, "amount")
                .ok_or_else(|| anyhow!("ingrediente sem amount / ingredient without amount"))?,
        });
    }

    let mut results = Vec::new();
    for entry in list(proto.get("results")) {
        let entry = entry
            .as_object()
            .ok_or_else(|| anyhow!("produto inválido / invalid product"))?;
        results.push(Product {
            kind: resource_kind(entry)?,
            name: required_str(entry, "name")?.to_owned(),
            amount: f64_field(entry, "amount"),
            amount_min: f64_field(entry, "amount_min"),
            amount_max: f64_field(entry, "amount_max"),
            probability: product_probability(entry),
            extra_count_fraction: f64_field(entry, "extra_count_fraction").unwrap_or(0.0),
            ignored_by_productivity: f64_field(entry, "ignored_by_productivity").unwrap_or(0.0),
            temperature: f64_field(entry, "temperature"),
        });
    }

    Ok(Recipe {
        name: name.to_owned(),
        categories: recipe_categories(proto),
        energy_required: f64_field(proto, "energy_required").unwrap_or(0.5),
        ingredients,
        results,
        main_product: str_field(proto, "main_product")
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        allow_productivity: bool_field(proto, "allow_productivity").unwrap_or(false),
        maximum_productivity: f64_field(proto, "maximum_productivity")
            .unwrap_or(bf_data::DEFAULT_MAXIMUM_PRODUCTIVITY),
        emissions_multiplier: f64_field(proto, "emissions_multiplier").unwrap_or(1.0),
        enabled: bool_field(proto, "enabled").unwrap_or(true),
        hidden: bool_field(proto, "hidden").unwrap_or(false),
        subgroup: str_field(proto, "subgroup").unwrap_or_default().to_owned(),
        order: str_field(proto, "order").unwrap_or_default().to_owned(),
    })
}

fn resource_kind(entry: &Object) -> Result<ResourceKind> {
    match str_field(entry, "type") {
        Some("item") | None => Ok(ResourceKind::Item),
        Some("fluid") => Ok(ResourceKind::Fluid),
        Some(other) => bail!("tipo de recurso desconhecido / unknown resource type {other:?}"),
    }
}

fn entity(name: &str, prototype: &str, proto: &Object) -> Result<Entity> {
    let collision_box = collision_box(proto)?;
    let size = match (
        u32_field(proto, "tile_width"),
        u32_field(proto, "tile_height"),
    ) {
        (Some(width), Some(height)) => TileSize { width, height },
        _ => size_from_box(&collision_box),
    };

    let energy_source = match proto.get("energy_source") {
        Some(Value::Object(source)) => Some(energy_source(source)?),
        _ => None,
    };
    let energy_usage_w = match str_field(proto, "energy_usage") {
        Some(value) => Some(parse_si(value, Quantity::Power)?),
        None => None,
    };

    Ok(Entity {
        name: name.to_owned(),
        prototype: prototype.to_owned(),
        size,
        collision_box,
        placed_by: Vec::new(),
        energy_source,
        energy_usage_w,
        fluid_boxes: fluid_boxes(proto)?,
        kind: entity_kind(prototype, proto)?,
    })
}

fn collision_box(proto: &Object) -> Result<[[f64; 2]; 2]> {
    let corners = list(proto.get("collision_box"));
    let point = |value: Option<&&Value>| -> Result<[f64; 2]> {
        let coords = list(value.copied());
        match coords.as_slice() {
            [x, y] => Ok([
                x.as_f64().ok_or_else(|| anyhow!("collision_box x"))?,
                y.as_f64().ok_or_else(|| anyhow!("collision_box y"))?,
            ]),
            _ => bail!("collision_box inválida / invalid collision_box"),
        }
    };
    Ok([point(corners.first())?, point(corners.get(1))?])
}

// PT: Tamanho em tiles a partir da caixa de colisão, arredondando para cima (mínimo 1).
//     O epsilon evita que 3.0000001 vire 4.
// EN: Size in tiles from the collision box, rounding up (at least 1).
//     The epsilon keeps 3.0000001 from becoming 4.
fn size_from_box(collision_box: &[[f64; 2]; 2]) -> TileSize {
    let [[x1, y1], [x2, y2]] = *collision_box;
    let tiles = |length: f64| -> u32 {
        let rounded = (length - 1e-6).ceil();
        if rounded < 1.0 { 1 } else { rounded as u32 }
    };
    TileSize {
        width: tiles(x2 - x1),
        height: tiles(y2 - y1),
    }
}

fn energy_source(source: &Object) -> Result<EnergySource> {
    let kind = match str_field(source, "type") {
        Some("electric") => EnergySourceKind::Electric,
        Some("burner") => EnergySourceKind::Burner,
        Some("heat") => EnergySourceKind::Heat,
        Some("fluid") => EnergySourceKind::Fluid,
        Some("void") => EnergySourceKind::Void,
        other => bail!("energy_source desconhecida / unknown energy_source {other:?}"),
    };

    let mut emissions_per_minute = BTreeMap::new();
    if let Some(Value::Object(emissions)) = source.get("emissions_per_minute") {
        for (pollutant, value) in emissions {
            if let Some(value) = value.as_f64() {
                emissions_per_minute.insert(pollutant.clone(), value);
            }
        }
    }

    let mut fuel_categories: Vec<String> = list(source.get("fuel_categories"))
        .iter()
        .filter_map(|category| category.as_str().map(str::to_owned))
        .collect();
    if kind == EnergySourceKind::Burner && fuel_categories.is_empty() {
        fuel_categories.push("chemical".to_owned());
    }

    let power = |key: &str| -> Result<Option<f64>> {
        str_field(source, key)
            .map(|value| parse_si(value, Quantity::Power))
            .transpose()
            .map_err(Into::into)
    };

    Ok(EnergySource {
        kind,
        drain_w: power("drain")?,
        emissions_per_minute,
        fuel_categories,
        effectivity: f64_field(source, "effectivity").unwrap_or(1.0),
        usage_priority: str_field(source, "usage_priority").map(str::to_owned),
        buffer_capacity_j: str_field(source, "buffer_capacity")
            .map(|value| parse_si(value, Quantity::Energy))
            .transpose()?,
        input_flow_limit_w: power("input_flow_limit")?,
        output_flow_limit_w: power("output_flow_limit")?,
    })
}

fn entity_kind(prototype: &str, proto: &Object) -> Result<EntityKind> {
    let speed =
        || f64_field(proto, "speed").ok_or_else(|| anyhow!("speed ausente / missing speed"));
    let module_slots = || u32_field(proto, "module_slots").unwrap_or(0);
    let strings = |key: &str| -> Vec<String> {
        list(proto.get(key))
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    };

    let kind = match prototype {
        "assembling-machine" | "furnace" | "rocket-silo" => {
            let receiver = proto.get("effect_receiver").and_then(Value::as_object);
            let receiver_flag =
                |key: &str| receiver.and_then(|r| bool_field(r, key)).unwrap_or(true);
            EntityKind::CraftingMachine {
                crafting_speed: f64_field(proto, "crafting_speed").unwrap_or(1.0),
                crafting_categories: strings("crafting_categories"),
                module_slots: module_slots(),
                allowed_effects: strings("allowed_effects"),
                uses_module_effects: receiver_flag("uses_module_effects"),
                uses_beacon_effects: receiver_flag("uses_beacon_effects"),
                fixed_recipe: str_field(proto, "fixed_recipe")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            }
        }
        "beacon" => EntityKind::Beacon {
            supply_area_distance: f64_field(proto, "supply_area_distance").unwrap_or(0.0),
            distribution_effectivity: f64_field(proto, "distribution_effectivity").unwrap_or(1.0),
            profile: list(proto.get("profile"))
                .iter()
                .filter_map(|v| v.as_f64())
                .collect(),
            module_slots: module_slots(),
            allowed_effects: strings("allowed_effects"),
        },
        "transport-belt" => EntityKind::TransportBelt { speed: speed()? },
        "underground-belt" => EntityKind::UndergroundBelt {
            speed: speed()?,
            max_distance: u32_field(proto, "max_distance").unwrap_or(5),
        },
        "splitter" => EntityKind::Splitter { speed: speed()? },
        "loader" | "loader-1x1" => EntityKind::Loader { speed: speed()? },
        "inserter" => EntityKind::Inserter {
            pickup_position: point(proto.get("pickup_position"))?,
            insert_position: point(proto.get("insert_position"))?,
            rotation_speed: f64_field(proto, "rotation_speed").unwrap_or(0.0),
            extension_speed: f64_field(proto, "extension_speed").unwrap_or(0.0),
            energy_per_movement_j: energy_or_zero(proto, "energy_per_movement")?,
            energy_per_rotation_j: energy_or_zero(proto, "energy_per_rotation")?,
            bulk: bool_field(proto, "bulk").unwrap_or(false),
        },
        "electric-pole" => EntityKind::ElectricPole {
            supply_area_distance: f64_field(proto, "supply_area_distance").unwrap_or(0.0),
            maximum_wire_distance: f64_field(proto, "maximum_wire_distance").unwrap_or(0.0),
        },
        "container" | "logistic-container" | "infinity-container" => EntityKind::Container {
            inventory_size: u32_field(proto, "inventory_size").unwrap_or(0),
            logistic_mode: str_field(proto, "logistic_mode").map(str::to_owned),
        },
        "boiler" => EntityKind::Boiler {
            target_temperature: f64_field(proto, "target_temperature").unwrap_or(0.0),
            energy_consumption_w: match str_field(proto, "energy_consumption") {
                Some(value) => parse_si(value, Quantity::Power)?,
                None => 0.0,
            },
        },
        "generator" => EntityKind::Generator {
            fluid_usage_per_tick: f64_field(proto, "fluid_usage_per_tick").unwrap_or(0.0),
            maximum_temperature: f64_field(proto, "maximum_temperature").unwrap_or(0.0),
            effectivity: f64_field(proto, "effectivity").unwrap_or(1.0),
        },
        "solar-panel" => EntityKind::SolarPanel {
            production_w: match str_field(proto, "production") {
                Some(value) => parse_si(value, Quantity::Power)?,
                None => 0.0,
            },
        },
        "accumulator" => EntityKind::Accumulator,
        "reactor" => EntityKind::Reactor {
            consumption_w: match str_field(proto, "consumption") {
                Some(value) => parse_si(value, Quantity::Power)?,
                None => 0.0,
            },
            neighbour_bonus: f64_field(proto, "neighbour_bonus").unwrap_or(1.0),
        },
        "mining-drill" => EntityKind::MiningDrill {
            mining_speed: f64_field(proto, "mining_speed").unwrap_or(1.0),
            resource_categories: strings("resource_categories"),
            module_slots: module_slots(),
        },
        "lab" => EntityKind::Lab {
            researching_speed: f64_field(proto, "researching_speed").unwrap_or(1.0),
            module_slots: module_slots(),
            inputs: strings("inputs"),
        },
        "pipe" => EntityKind::Pipe,
        "pipe-to-ground" => {
            let fluid_box = proto.get("fluid_box").and_then(Value::as_object);
            let distance = fluid_box
                .map(|fluid_box| {
                    list(fluid_box.get("pipe_connections"))
                        .iter()
                        .filter_map(|connection| connection.as_object())
                        .filter_map(|connection| u32_field(connection, "max_underground_distance"))
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            EntityKind::PipeToGround {
                max_underground_distance: distance,
            }
        }
        "pump" => EntityKind::Pump {
            pumping_speed: f64_field(proto, "pumping_speed").unwrap_or(0.0),
        },
        "offshore-pump" => EntityKind::OffshorePump {
            pumping_speed: f64_field(proto, "pumping_speed").unwrap_or(0.0),
        },
        "storage-tank" => EntityKind::StorageTank,
        "constant-combinator" => EntityKind::ConstantCombinator,
        _ => EntityKind::Other,
    };
    Ok(kind)
}

// PT: Caixas de fluido de qualquer entidade, na ordem do jogo: a lista `fluid_boxes` das
//     máquinas (o índice casa com o `fluidbox_index` das receitas) e depois as caixas avulsas
//     (canos, bombas, tanques; na caldeira, a entrada de água e a saída de vapor).
// EN: Fluid boxes of any entity, in game order: the machines' `fluid_boxes` list (the index
//     matches the recipes' `fluidbox_index`) and then the single boxes (pipes, pumps, tanks;
//     for the boiler, the water input and the steam output).
fn fluid_boxes(proto: &Object) -> Result<Vec<FluidBox>> {
    let mut boxes = Vec::new();
    for fluid_box in list(proto.get("fluid_boxes")) {
        if let Some(fluid_box) = fluid_box.as_object() {
            boxes.push(parse_fluid_box(fluid_box)?);
        }
    }
    for key in ["fluid_box", "input_fluid_box", "output_fluid_box"] {
        if let Some(fluid_box) = proto.get(key).and_then(Value::as_object) {
            boxes.push(parse_fluid_box(fluid_box)?);
        }
    }
    Ok(boxes)
}

fn parse_fluid_box(fluid_box: &Object) -> Result<FluidBox> {
    let mut pipe_connections = Vec::new();
    for connection in list(fluid_box.get("pipe_connections")) {
        let Some(connection) = connection.as_object() else {
            continue;
        };
        pipe_connections.push(PipeConnection {
            position: match connection.get("position") {
                Some(value) => Some(point(Some(value))?),
                None => None,
            },
            direction: u32_field(connection, "direction").and_then(|d| u8::try_from(d).ok()),
            flow_direction: str_field(connection, "flow_direction")
                .unwrap_or("input-output")
                .to_owned(),
            connection_type: str_field(connection, "connection_type")
                .unwrap_or("normal")
                .to_owned(),
            max_underground_distance: u32_field(connection, "max_underground_distance"),
        });
    }
    Ok(FluidBox {
        production_type: str_field(fluid_box, "production_type")
            .unwrap_or("none")
            .to_owned(),
        volume: f64_field(fluid_box, "volume").unwrap_or(0.0),
        filter: str_field(fluid_box, "filter").map(str::to_owned),
        pipe_connections,
    })
}

fn energy_or_zero(proto: &Object, key: &str) -> Result<f64> {
    match str_field(proto, key) {
        Some(value) => Ok(parse_si(value, Quantity::Energy)?),
        None => Ok(0.0),
    }
}

// PT: Ponto `[x, y]` ou `{x, y}`, as duas formas que o jogo aceita.
// EN: A `[x, y]` or `{x, y}` point, the two forms the game accepts.
fn point(value: Option<&Value>) -> Result<[f64; 2]> {
    match value {
        Some(Value::Array(coords)) => match coords.as_slice() {
            [x, y] => Ok([
                x.as_f64().ok_or_else(|| anyhow!("x"))?,
                y.as_f64().ok_or_else(|| anyhow!("y"))?,
            ]),
            _ => bail!("ponto inválido / invalid point"),
        },
        Some(Value::Object(coords)) => Ok([
            f64_field(coords, "x").ok_or_else(|| anyhow!("x"))?,
            f64_field(coords, "y").ok_or_else(|| anyhow!("y"))?,
        ]),
        _ => bail!("ponto ausente / missing point"),
    }
}

// PT: Lista Lua: um array JSON, ou `{}` quando a tabela é vazia (o dump não distingue).
// EN: Lua list: a JSON array, or `{}` when the table is empty (the dump cannot tell them apart).
fn list(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(values)) => values.iter().collect(),
        _ => Vec::new(),
    }
}

fn str_field<'a>(object: &'a Object, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn required_str<'a>(object: &'a Object, key: &str) -> Result<&'a str> {
    str_field(object, key).ok_or_else(|| anyhow!("campo {key:?} ausente / missing field {key:?}"))
}

fn f64_field(object: &Object, key: &str) -> Option<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn u32_field(object: &Object, key: &str) -> Option<u32> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn bool_field(object: &Object, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // PT: Fixture sintético no formato do dump, com só o necessário para cada caso.
    // EN: Synthetic fixture in the dump format, with only what each case needs.
    fn fixture() -> Value {
        json!({
            "item": {
                "iron-plate": { "type": "item", "name": "iron-plate", "stack_size": 100 },
                "iron-gear-wheel": { "type": "item", "name": "iron-gear-wheel", "stack_size": 100 },
                "coal": { "type": "item", "name": "coal", "stack_size": 50,
                          "fuel_categories": ["chemical"], "fuel_value": "4MJ" },
                "wood": { "type": "item", "name": "wood", "stack_size": 100,
                          "fuel_category": "chemical", "fuel_value": "2MJ" },
                "assembling-machine-2": { "type": "item", "name": "assembling-machine-2",
                          "stack_size": 50, "place_result": "assembling-machine-2" },
                "concrete": { "type": "item", "name": "concrete", "stack_size": 100,
                          "place_as_tile": { "result": "concrete", "condition_size": 1 } }
            },
            "module": {
                "speed-module": { "type": "module", "name": "speed-module", "stack_size": 50,
                    "category": "speed", "tier": 1,
                    "effect": { "speed": 0.2, "consumption": 0.5, "quality": -0.1 } }
            },
            "fluid": {
                "water": { "type": "fluid", "name": "water", "default_temperature": 15,
                           "max_temperature": 100, "heat_capacity": "0.2kJ" }
            },
            "recipe": {
                "iron-gear-wheel": {
                    "type": "recipe", "name": "iron-gear-wheel",
                    "ingredients": [{ "type": "item", "name": "iron-plate", "amount": 2 }],
                    "results": [{ "type": "item", "name": "iron-gear-wheel", "amount": 1 }],
                    "allow_productivity": true
                },
                "uranium-processing": {
                    "type": "recipe", "name": "uranium-processing", "categories": ["centrifuging"],
                    "ingredients": [{ "type": "item", "name": "uranium-ore", "amount": 10 }],
                    "results": [
                        { "type": "item", "name": "uranium-235", "amount": 1,
                          "shared_probability": { "min": 0, "max": 0.007 } },
                        { "type": "item", "name": "uranium-238", "amount": 1,
                          "shared_probability": { "min": 0.007, "max": 1 } }
                    ]
                },
                "legacy": {
                    "type": "recipe", "name": "legacy", "category": "smelting",
                    "ingredients": [{ "type": "item", "name": "iron-plate", "amount": 1 }],
                    "results": [{ "type": "item", "name": "iron-gear-wheel", "amount": 1,
                                  "probability": 0.5 }]
                },
                "mixed": {
                    "type": "recipe", "name": "mixed", "categories": ["crafting", "smelting"],
                    "ingredients": [{ "type": "item", "name": "iron-plate", "amount": 1 }],
                    "results": [{ "type": "item", "name": "iron-gear-wheel", "amount": 1,
                                  "independent_probability": 0.5,
                                  "shared_probability": { "min": 0, "max": 0.2 } },
                                { "type": "item", "name": "iron-plate", "amount": 1,
                                  "shared_probability": { "min": -0.5, "max": 0.5 } }]
                },
                "empty": { "type": "recipe", "name": "empty", "ingredients": {}, "results": {} }
            },
            "assembling-machine": {
                "assembling-machine-2": {
                    "type": "assembling-machine", "name": "assembling-machine-2",
                    "collision_box": [[-1.2, -1.2], [1.2, 1.2]],
                    "crafting_speed": 0.75, "crafting_categories": ["crafting"],
                    "energy_usage": "150kW", "module_slots": 2,
                    "energy_source": { "type": "electric", "usage_priority": "secondary-input",
                                       "emissions_per_minute": { "pollution": 3 } },
                    "fluid_boxes": [{ "production_type": "input", "volume": 1000,
                        "pipe_connections": [{ "flow_direction": "input", "direction": 0,
                                               "position": [0, -1] }] }]
                }
            },
            "corpse": {
                "assembling-machine-2-remnants": { "type": "corpse",
                    "name": "assembling-machine-2-remnants",
                    "collision_box": [[-1.2, -1.2], [1.2, 1.2]] }
            }
        })
    }

    #[test]
    fn normalizes_items_fluids_and_recipes() {
        let data = normalize(&fixture(), "2.1.23").unwrap();
        assert_eq!(data.game_version, "2.1.23");

        let coal = &data.items["coal"];
        assert_eq!(coal.fuel.as_ref().unwrap().value_j, 4_000_000.0);
        let module = data.items["speed-module"].module.as_ref().unwrap();
        assert_eq!(module.effect.speed, 0.2);
        assert_eq!(module.effect.quality, -0.1);
        assert_eq!(
            data.items["concrete"].places_tile.as_deref(),
            Some("concrete")
        );
        assert_eq!(
            data.tiles["concrete"].placed_by,
            vec!["concrete".to_owned()]
        );

        assert_eq!(data.fluids["water"].heat_capacity_j, 200.0);

        let gear = &data.recipes["iron-gear-wheel"];
        assert_eq!(gear.energy_required, 0.5);
        assert_eq!(gear.categories, vec!["crafting".to_owned()]);
        let uranium = &data.recipes["uranium-processing"];
        assert_eq!(uranium.categories, vec!["centrifuging".to_owned()]);
        assert!((uranium.results[0].probability - 0.007).abs() < 1e-12);
        assert!((uranium.results[1].probability - 0.993).abs() < 1e-12);

        // PT: Formato antigo (2.0) ainda lido. EN: Old (2.0) format still read.
        let legacy = &data.recipes["legacy"];
        assert_eq!(legacy.categories, vec!["smelting".to_owned()]);
        assert_eq!(legacy.results[0].probability, 0.5);
        assert_eq!(
            data.items["wood"].fuel.as_ref().unwrap().categories,
            vec!["chemical".to_owned()]
        );
        // PT: Independente × faixa compartilhada; a faixa é recortada em [0, 1).
        // EN: Independent × shared range; the range is clipped to [0, 1).
        let mixed = &data.recipes["mixed"];
        assert_eq!(mixed.categories.len(), 2);
        assert!((mixed.results[0].probability - 0.1).abs() < 1e-12);
        assert!((mixed.results[1].probability - 0.5).abs() < 1e-12);
        assert!(gear.enabled);
        assert_eq!(gear.ingredients[0].amount, 2.0);
        assert!(data.recipes["empty"].ingredients.is_empty());
    }

    #[test]
    fn keeps_only_buildable_entities() {
        let data = normalize(&fixture(), "2.1.23").unwrap();
        assert_eq!(data.entities.len(), 1);

        let machine = &data.entities["assembling-machine-2"];
        assert_eq!(
            machine.size,
            TileSize {
                width: 3,
                height: 3
            }
        );
        assert_eq!(machine.energy_usage_w, Some(150_000.0));
        assert_eq!(machine.placed_by, vec!["assembling-machine-2".to_owned()]);
        let source = machine.energy_source.as_ref().unwrap();
        assert_eq!(source.emissions_per_minute["pollution"], 3.0);
        match &machine.kind {
            EntityKind::CraftingMachine {
                crafting_speed,
                uses_module_effects,
                ..
            } => {
                assert_eq!(*crafting_speed, 0.75);
                assert!(*uses_module_effects);
                assert_eq!(
                    machine.fluid_boxes[0].pipe_connections[0].position,
                    Some([0.0, -1.0])
                );
                assert_eq!(
                    machine.fluid_boxes[0].pipe_connections[0].connection_type,
                    "normal"
                );
            }
            other => panic!("tipo errado / wrong kind: {other:?}"),
        }
    }

    #[test]
    fn computes_tile_sizes() {
        assert_eq!(
            size_from_box(&[[-0.15, -0.15], [0.15, 0.15]]),
            TileSize {
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            size_from_box(&[[-0.7, -0.7], [0.7, 0.7]]),
            TileSize {
                width: 2,
                height: 2
            }
        );
        assert_eq!(
            size_from_box(&[[-1.29, -0.79], [1.29, 0.79]]),
            TileSize {
                width: 3,
                height: 2
            }
        );
        assert_eq!(
            size_from_box(&[[-2.4, -2.4], [2.4, 2.4]]),
            TileSize {
                width: 5,
                height: 5
            }
        );
        assert_eq!(
            size_from_box(&[[-0.9, -0.4], [0.9, 0.4]]),
            TileSize {
                width: 2,
                height: 1
            }
        );
    }
}
