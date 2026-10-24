//! PT: Entrada comum das duas simulações: o que cada máquina consome e produz por ciclo, a
//!     capacidade de cada insersor (tabela medida no jogo), as entradas de esteira (marcadas por
//!     combinador constante ou deduzidas) e as saídas (pontas de esteira sem nenhum insersor
//!     pegando no caminho até elas). Fluidos não passam por esteira e são supostos disponíveis.
//! EN: Common input of both simulations: what each machine consumes and produces per cycle, the
//!     capacity of each inserter (table measured in-game), the belt inputs (marked by a constant
//!     combinator or inferred) and the outputs (belt ends with no inserter picking along the path
//!     to them). Fluids do not travel on belts and are assumed available.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use bf_calc::{CalcReport, InserterResearch};
use bf_data::{EnergySourceKind, EntityKind, GameData, ResourceKind};
use bf_world::belts::{BeltNetwork, Output, TileKind};
use bf_world::inserters::Endpoint;
use bf_world::{Topology, World};
use serde::Deserialize;

/// PT: Tabela medida no jogo por `bf-validate measure-inserters`.
/// EN: Table measured in-game by `bf-validate measure-inserters`.
const THROUGHPUT_TABLE: &str =
    include_str!("../../../validation/measurements/inserter-throughput.json");

/// PT: Máquina com o que entra e sai por ciclo (só itens) e o máximo de ciclos por segundo.
/// EN: Machine with what goes in and out per cycle (items only) and the maximum cycles per second.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineIo {
    pub entity: usize,
    pub entity_number: u64,
    pub max_crafts: f64,
    pub inputs: Vec<(usize, f64)>,
    pub outputs: Vec<(usize, f64)>,
}

/// PT: Ponta de um insersor já resolvida. EN: An inserter end already resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Nothing,
    /// PT: Máquina, pela posição em `SimInput::machines`. EN: Machine, by position in `SimInput::machines`.
    Machine(usize),
    /// PT: Baú ou outra entidade sem receita: fonte e destino ilimitados.
    /// EN: Chest or another recipe-less entity: unlimited source and destination.
    Storage,
    Belt {
        tile: usize,
        lane: Option<usize>,
    },
}

/// PT: Insersor com as pontas e a capacidade. EN: Inserter with its ends and capacity.
#[derive(Debug, Clone, PartialEq)]
pub struct InserterIo {
    pub entity: usize,
    pub entity_number: u64,
    pub name: String,
    pub pickup: End,
    pub drop: End,
    /// PT: Itens por segundo no caso medido mais próximo. EN: Items per second in the closest measured case.
    pub capacity: f64,
    /// PT: Ticks de meia-volta e itens por mão, para a simulação por tick.
    /// EN: Half-swing ticks and items per hand, for the tick simulation.
    pub half_swing_ticks: u32,
    pub hand_size: u32,
    /// PT: Ticks a menos (ou a mais) na volta quando pega de esteira: o braço alcança o item antes
    ///     de terminar o giro. Sai da tabela medida (sem pesquisa, esteira → baú).
    /// EN: Ticks fewer (or more) on the way back when picking from a belt: the arm reaches the
    ///     item before finishing the swing. Comes from the measured table (no research, belt → chest).
    pub belt_pickup_saving: i32,
}

/// PT: Entrada de esteira: `items` por faixa; vazio quer dizer "o que for pedido" (deduzida).
/// EN: Belt input: `items` per lane; empty means "whatever is requested" (inferred).
#[derive(Debug, Clone, PartialEq)]
pub struct BeltSource {
    pub tile: usize,
    pub items: [Vec<usize>; 2],
}

/// PT: Tudo o que as simulações precisam. EN: Everything the simulations need.
#[derive(Debug, Clone)]
pub struct SimInput<'a> {
    pub belts: &'a BeltNetwork,
    pub items: Vec<String>,
    pub machines: Vec<MachineIo>,
    pub inserters: Vec<InserterIo>,
    pub sources: Vec<BeltSource>,
    /// PT: Tiles de saída (pontas que esvaziam). EN: Output tiles (ends that drain).
    pub sinks: BTreeSet<usize>,
    /// PT: Ordem de processamento dos tiles (de quem manda para quem recebe).
    /// EN: Tile processing order (from senders to receivers).
    pub order: Vec<usize>,
}

impl SimInput<'_> {
    pub fn item_index(&self, name: &str) -> Option<usize> {
        self.items.iter().position(|item| item == name)
    }
}

#[derive(Deserialize)]
struct Measured {
    inserter: String,
    from: String,
    to: String,
    #[serde(default)]
    belt: Option<String>,
    research: String,
    items_per_second: f64,
}

/// PT: Monta a entrada das simulações. EN: Builds the simulations' input.
pub fn build<'a>(
    data: &GameData,
    world: &World,
    topology: &'a Topology,
    report: &CalcReport,
    research: InserterResearch,
    fuel: &str,
) -> SimInput<'a> {
    let belts = &topology.belts;
    let mut items: Vec<String> = Vec::new();

    // PT: Máquinas a partir do relatório do cálculo (receita, efeito e ciclos já resolvidos).
    // EN: Machines from the calculation report (recipe, effect and cycles already resolved).
    let by_number: HashMap<u64, usize> = world
        .entities
        .iter()
        .enumerate()
        .map(|(slot, e)| (e.entity_number, slot))
        .collect();
    let fuel_value = data
        .items
        .get(fuel)
        .and_then(|item| item.fuel.as_ref())
        .map_or(0.0, |f| f.value_j);
    let mut machines = Vec::new();
    let mut machine_of: HashMap<usize, usize> = HashMap::new();
    for machine in &report.machines {
        let Some(&entity) = by_number.get(&machine.entity_number) else {
            continue;
        };
        let recipe = machine.recipe.as_ref().and_then(|r| data.recipes.get(r));
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        if let Some(recipe) = recipe {
            for ingredient in recipe
                .ingredients
                .iter()
                .filter(|i| i.kind == ResourceKind::Item)
            {
                inputs.push((intern(&mut items, &ingredient.name), ingredient.amount));
            }
            for product in recipe
                .results
                .iter()
                .filter(|p| p.kind == ResourceKind::Item)
            {
                let expected = product.expected_amount();
                let productive = (expected - product.ignored_by_productivity).max(0.0);
                let amount = expected + productive * machine.effect.productivity;
                outputs.push((intern(&mut items, &product.name), amount));
            }
            let burner = data
                .entities
                .get(&machine.name)
                .and_then(|e| e.energy_source.as_ref())
                .filter(|s| s.kind == EnergySourceKind::Burner);
            if let Some(source) = burner
                && fuel_value > 0.0
                && machine.crafts_per_second > 0.0
            {
                let per_second = machine.power_w / source.effectivity.max(1e-9) / fuel_value;
                inputs.push((
                    intern(&mut items, fuel),
                    per_second / machine.crafts_per_second,
                ));
            }
        }
        machine_of.insert(entity, machines.len());
        machines.push(MachineIo {
            entity,
            entity_number: machine.entity_number,
            max_crafts: if recipe.is_some() {
                machine.crafts_per_second
            } else {
                0.0
            },
            inputs,
            outputs,
        });
    }

    let table = table();
    let names = belt_names(data);
    let research_name = match research {
        InserterResearch::None => "none",
        InserterResearch::All => "all",
    };
    let resolve = |endpoint: Endpoint| match endpoint {
        Endpoint::Nothing => End::Nothing,
        Endpoint::Belt { tile, lane } => End::Belt { tile, lane },
        Endpoint::Entity { entity } => machine_of
            .get(&entity)
            .map_or(End::Storage, |m| End::Machine(*m)),
    };
    let mut inserters = Vec::new();
    for link in &topology.inserters {
        let Some(placed) = world.entities.get(link.entity) else {
            continue;
        };
        let Some(EntityKind::Inserter {
            rotation_speed,
            bulk,
            ..
        }) = data.entities.get(&placed.name).map(|p| &p.kind)
        else {
            continue;
        };
        let pickup = resolve(link.pickup);
        let drop = resolve(link.drop);
        let half_swing_ticks = half_swing(*rotation_speed);
        let hand_size = match (research, bulk) {
            (InserterResearch::None, _) => 1,
            (InserterResearch::All, true) => 12,
            (InserterResearch::All, false) => 3,
        };
        let capacity = lookup(
            table,
            &placed.name,
            pickup,
            drop,
            belts,
            &names,
            research_name,
        )
        .unwrap_or(f64::from(hand_size) * 60.0 / (2.0 * f64::from(half_swing_ticks.max(1))));
        let belt_pickup_saving = table
            .iter()
            .find(|m| {
                m.inserter == placed.name
                    && m.research == "none"
                    && m.from == "belt"
                    && m.to == "chest"
                    && m.belt.as_deref() == Some("transport-belt")
            })
            .filter(|m| m.items_per_second > 0.0)
            .map_or(0, |m| {
                (2.0 * f64::from(half_swing_ticks) - 60.0 / m.items_per_second).round() as i32
            });
        inserters.push(InserterIo {
            entity: link.entity,
            entity_number: placed.entity_number,
            belt_pickup_saving,
            name: placed.name.clone(),
            pickup,
            drop,
            capacity: if topology.power.powered(link.entity) {
                capacity
            } else {
                0.0
            },
            half_swing_ticks,
            hand_size,
        });
    }

    let order = topology.order.clone();
    let sinks = sinks(world, topology);
    let sources = sources(topology, &inserters, &machines, &mut items);
    SimInput {
        belts,
        items,
        machines,
        inserters,
        sources,
        sinks,
        order,
    }
}

fn intern(items: &mut Vec<String>, name: &str) -> usize {
    items.iter().position(|i| i == name).unwrap_or_else(|| {
        items.push(name.to_owned());
        items.len() - 1
    })
}

/// PT: Ticks de meia-volta do braço: o jogo soma a velocidade de giro a cada tick até chegar a
///     meia volta, e o último tick já conta para a volta seguinte (conferido na tabela medida).
/// EN: Half-swing ticks of the arm: the game adds the rotation speed every tick until reaching
///     half a turn, and the last tick already counts for the next turn (checked in the measured table).
pub fn half_swing(rotation_speed: f64) -> u32 {
    if rotation_speed <= 0.0 {
        return u32::MAX;
    }
    let ticks = (0.5 / rotation_speed - 1e-9).ceil() - 1.0;
    ticks.clamp(1.0, 1e6) as u32
}

// PT: A tabela medida, lida uma vez por processo. EN: The measured table, read once per process.
fn table() -> &'static [Measured] {
    static TABLE: OnceLock<Vec<Measured>> = OnceLock::new();
    TABLE.get_or_init(|| serde_json::from_str(THROUGHPUT_TABLE).unwrap_or_default())
}

// PT: Velocidade → nome da esteira comum, para achar o caso medido. EN: Speed → regular belt name, to find the measured case.
fn belt_names(data: &GameData) -> Vec<(f64, String)> {
    data.entities
        .iter()
        .filter_map(|(name, e)| match e.kind {
            EntityKind::TransportBelt { speed } => Some((speed, name.clone())),
            _ => None,
        })
        .collect()
}

fn belt_name(belts: &BeltNetwork, tile: usize, names: &[(f64, String)]) -> Option<String> {
    let speed = belts.tiles.get(tile)?.speed;
    names
        .iter()
        .find(|(s, _)| (s - speed).abs() < 1e-9)
        .map(|(_, name)| name.clone())
}

fn lookup(
    table: &[Measured],
    inserter: &str,
    pickup: End,
    drop: End,
    belts: &BeltNetwork,
    names: &[(f64, String)],
    research: &str,
) -> Option<f64> {
    let kind = |end: End| match end {
        End::Belt { tile, .. } => ("belt", belt_name(belts, tile, names)),
        _ => ("chest", None),
    };
    let (from, from_belt) = kind(pickup);
    let (to, to_belt) = kind(drop);
    let find = |from: &str, to: &str, belt: Option<&String>| {
        table
            .iter()
            .find(|m| {
                m.inserter == inserter
                    && m.research == research
                    && m.from == from
                    && m.to == to
                    && m.belt.as_ref() == belt
            })
            .map(|m| m.items_per_second)
            .filter(|v| v.is_finite())
    };
    match (from, to) {
        // PT: Esteira para esteira: o menor dos dois casos medidos. EN: Belt to belt: the smaller of both measured cases.
        ("belt", "belt") => {
            let a = find("belt", "chest", from_belt.as_ref())?;
            let b = find("chest", "belt", to_belt.as_ref())?;
            Some(a.min(b))
        }
        ("belt", _) => find("belt", "chest", from_belt.as_ref()),
        (_, "belt") => find("chest", "belt", to_belt.as_ref()),
        _ => find("chest", "chest", None),
    }
}

// PT: Saída = esteira comum na ponta, sem nada à frente no mundo e sem insersor pegando em todo
//     o caminho até ela (a mesma regra do validador no jogo). Uma passada na ordem das esteiras.
// EN: Output = a regular belt at the end, with nothing ahead in the world and no inserter picking
//     along the whole path to it (the same rule as the in-game validator). One pass in belt order.
fn sinks(world: &World, topology: &Topology) -> BTreeSet<usize> {
    let belts = &topology.belts;
    let mut picked = vec![false; belts.tiles.len()];
    for tile in topology.pickers_by_tile.keys() {
        if let Some(slot) = picked.get_mut(*tile) {
            *slot = true;
        }
    }
    for &tile in &topology.order {
        if picked.get(tile).copied().unwrap_or(false) {
            for next in belts.successors(tile) {
                if let Some(slot) = picked.get_mut(next) {
                    *slot = true;
                }
            }
        }
    }
    belts
        .tiles
        .iter()
        .enumerate()
        .filter(|(index, tile)| {
            let (x, y) = bf_world::dir::step((tile.x, tile.y), tile.dir, 1);
            tile.output == Output::None
                && tile.kind == TileKind::Belt
                && !picked.get(*index).copied().unwrap_or(true)
                && world.entity_at(x, y).is_none()
        })
        .map(|(index, _)| index)
        .collect()
}

// PT: Entradas: cabeças de esteira. Com marcador (combinador), a faixa da esquerda leva o 1º item
//     e a da direita o 2º (ou o 1º de novo); havendo algum marcador, só as marcadas entram. Sem
//     marcador, a cabeça é entrada quando nenhum insersor solta no caminho dela, e oferece só o
//     que as máquinas adiante pedem. Duas passadas, na ordem das esteiras ao contrário.
// EN: Inputs: belt heads. With a marker (combinator), the left lane carries the 1st item and the
//     right one the 2nd (or the 1st again); if any marker exists, only marked heads count.
//     Without a marker, a head is an input when no inserter drops along its path, and it only
//     offers what the machines further on ask for. Two passes, in reverse belt order.
fn sources(
    topology: &Topology,
    inserters: &[InserterIo],
    machines: &[MachineIo],
    items: &mut Vec<String>,
) -> Vec<BeltSource> {
    let belts = &topology.belts;
    let count = belts.tiles.len();
    let mut dropped = vec![false; count];
    let mut wanted: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); count];
    for inserter in inserters {
        if let End::Belt { tile, .. } = inserter.drop
            && let Some(slot) = dropped.get_mut(tile)
        {
            *slot = true;
        }
        if let (End::Belt { tile, .. }, End::Machine(m)) = (inserter.pickup, inserter.drop)
            && let (Some(set), Some(machine)) = (wanted.get_mut(tile), machines.get(m))
        {
            set.extend(machine.inputs.iter().map(|(i, _)| *i));
        }
    }
    for &tile in topology.order.iter().rev() {
        let mut drop_ahead = dropped.get(tile).copied().unwrap_or(false);
        let mut ahead = BTreeSet::new();
        for next in belts.successors(tile) {
            drop_ahead |= dropped.get(next).copied().unwrap_or(false);
            if let Some(set) = wanted.get(next) {
                ahead.extend(set.iter().copied());
            }
        }
        if let Some(slot) = dropped.get_mut(tile) {
            *slot = drop_ahead;
        }
        if let Some(set) = wanted.get_mut(tile) {
            set.extend(ahead);
        }
    }
    let explicit = !topology.markers.is_empty();
    let mut result = Vec::new();
    for head in belts.heads() {
        let lanes = match topology.markers.get(&head) {
            Some(names) => {
                let first = names.first().cloned().unwrap_or_default();
                let second = names.get(1).cloned().unwrap_or_else(|| first.clone());
                [vec![intern(items, &first)], vec![intern(items, &second)]]
            }
            None if explicit || dropped.get(head).copied().unwrap_or(true) => continue,
            None => {
                let offered: Vec<usize> = wanted
                    .get(head)
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default();
                if offered.is_empty() {
                    continue;
                }
                [offered.clone(), offered]
            }
        };
        result.push(BeltSource {
            tile: head,
            items: lanes,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_table_is_read_and_complete() {
        let table = table();
        assert!(table.len() >= 60, "{}", table.len());
        assert!(
            table
                .iter()
                .all(|m| m.items_per_second.is_finite() && m.items_per_second > 0.0)
        );
    }

    #[test]
    fn half_swing_matches_the_measured_cycles() {
        assert_eq!(half_swing(0.014), 35);
        assert_eq!(half_swing(0.04), 12);
        assert_eq!(half_swing(0.02), 24);
        assert_eq!(half_swing(0.0), u32::MAX);
    }
}
