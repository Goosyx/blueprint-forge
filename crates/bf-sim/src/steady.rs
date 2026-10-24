//! PT: Simulação em regime estável: vazão por item em cada faixa, insersores com a capacidade
//!     medida no jogo e máquinas no máximo que as entradas e saídas deixam. Cada rodada tem duas
//!     passadas pelas esteiras, na ordem delas (quem está antes pega primeiro):
//!     - potencial: cada máquina pede o máximo que consegue usar, e o produtor oferece o que as
//!       entradas dele permitem; daí sai o ritmo possível de cada máquina pelas entradas;
//!     - real: cada máquina consome no ritmo da rodada anterior, e o que os produtos conseguem
//!       sair limita o ritmo.
//!     Esteira sem saída só aceita o que alguém mais adiante pega, como no jogo, em que ela enche
//!     e para. As rodadas repetem até nada mudar.
//! EN: Steady-state simulation: per-item throughput on each lane, inserters with the capacity
//!     measured in-game and machines at the most their inputs and outputs allow. Each round makes
//!     two passes over the belts, in belt order (whoever is earlier picks first):
//!     - potential: each machine asks for the most it can use, and the producer offers what its
//!       inputs allow; this gives each machine's possible pace from its inputs;
//!     - actual: each machine consumes at the previous round's pace, and how fast the products can
//!       leave limits the pace.
//!     A belt without an output only accepts what someone further on picks, as in the game, where
//!     it fills up and stops. Rounds repeat until nothing changes.

use std::collections::BTreeMap;

use bf_world::belts::{BeltNetwork, Output, Side, TileKind};

use crate::model::{End, SimInput};
use crate::{FlowReport, InserterFlow, LaneFlow, Limit, MachineFlow, SimError};

/// PT: Teto de células (faixas × itens). Até oito vetores desse tamanho convivem numa rodada,
///     então o pico fica em cerca de 8 × 2,5 milhões × 8 bytes = 160 MB.
/// EN: Cap on cells (lanes × items). Up to eight vectors of that size coexist in one round, so
///     the peak stays around 8 × 2.5 million × 8 bytes = 160 MB.
pub const MAX_CELLS: usize = 2_500_000;
const ROUNDS: usize = 500;
/// PT: Orçamento de trabalho (células visitadas, somando as rodadas). Passou dele, a simulação
///     para e devolve o que tem, marcada como não convergida.
/// EN: Work budget (visited cells, summed over rounds). Past it, the simulation stops and returns
///     what it has, marked as not converged.
pub const MAX_WORK: u64 = 400_000_000;
const STABLE_ROUNDS: usize = 8;

/// PT: Roda a simulação. EN: Runs the simulation.
pub fn simulate(input: &SimInput<'_>) -> Result<FlowReport, SimError> {
    let items = input.items.len().max(1);
    let lanes = input.belts.tiles.len() * 2;
    if lanes.saturating_mul(items) > MAX_CELLS {
        return Err(SimError::TooLarge);
    }
    let context = Context::new(input, items);
    let maximum: Vec<f64> = input.machines.iter().map(|m| m.max_crafts).collect();
    let mut potential = maximum.clone();
    let mut rate = maximum.clone();
    let mut absorb_potential = context.absorb(&vec![0.0; lanes * items]);
    let mut absorb_actual = absorb_potential.clone();
    let mut limits = vec![Limit::None; input.machines.len()];
    let mut converged = false;
    let mut rounds = 0;
    let mut stable = 0;
    // PT: Cada rodada visita as células cerca de 12 vezes (duas passadas, a capacidade de cada
    //     faixa e as duas contas de absorção) mais máquinas e insersores.
    // EN: Each round visits the cells about 12 times (two passes, each lane's capacity and both
    //     absorption sums) plus machines and inserters.
    let round_cost = (lanes * items * 12 + input.machines.len() + input.inserters.len()) as u64;
    let mut work: u64 = 0;
    for round in 0..ROUNDS {
        work = work.saturating_add(round_cost);
        if work > MAX_WORK {
            break;
        }
        rounds = round + 1;
        // PT: Passada potencial. EN: Potential pass.
        let a = context.pass(&maximum, &potential, &absorb_potential);
        absorb_potential = context.absorb(&a.demand_at);
        let mut next_potential = Vec::with_capacity(maximum.len());
        let mut input_limit: Vec<Option<usize>> = Vec::with_capacity(maximum.len());
        for (m, machine) in input.machines.iter().enumerate() {
            let mut best = machine.max_crafts;
            let mut short = None;
            for (slot, (item, per)) in machine.inputs.iter().enumerate() {
                if *per > 0.0 {
                    let can = cell(&a.got, m, slot) / per;
                    if can < best - 1e-12 {
                        best = can;
                        short = Some(*item);
                    }
                }
            }
            next_potential.push(best.max(0.0));
            input_limit.push(short);
        }

        // PT: Passada real. EN: Actual pass.
        let demand: Vec<f64> = rate
            .iter()
            .zip(&next_potential)
            .map(|(r, p)| r.min(*p))
            .collect();
        let b = context.pass(&demand, &next_potential, &absorb_actual);
        absorb_actual = context.absorb(&b.demand_at);
        let mut change: f64 = 0.0;
        for (m, machine) in input.machines.iter().enumerate() {
            let mut next = cell1(&next_potential, m);
            let mut limit = match input_limit.get(m).copied().flatten() {
                Some(item) => Limit::Input {
                    item: name(input, item),
                },
                None => Limit::None,
            };
            for (slot, (item, per)) in machine.outputs.iter().enumerate() {
                if *per <= 0.0 {
                    continue;
                }
                let can = cell(&b.removed, m, slot) / per;
                if can < next - 1e-12 {
                    next = can;
                    limit = Limit::Output {
                        item: name(input, *item),
                    };
                }
            }
            let next = next.clamp(0.0, machine.max_crafts);
            change = change.max((next - cell1(&rate, m)).abs());
            change = change.max((cell1(&next_potential, m) - cell1(&potential, m)).abs());
            if let Some(slot) = rate.get_mut(m) {
                *slot = next;
            }
            if let Some(slot) = limits.get_mut(m) {
                *slot = limit;
            }
        }
        potential = next_potential;
        // PT: Uma mudança leva uma rodada para atravessar cada elo da cadeia; só para depois de
        //     várias rodadas seguidas sem mudança.
        // EN: A change takes one round to cross each link of the chain; it only stops after
        //     several consecutive rounds without change.
        stable = if change < 1e-10 { stable + 1 } else { 0 };
        if stable >= STABLE_ROUNDS {
            converged = true;
            break;
        }
    }
    // PT: Passada final no ritmo encontrado, para o relatório. EN: Final pass at the pace found, for the report.
    let last = context.pass(&rate, &rate, &absorb_actual);
    Ok(context.report(&rate, &limits, &last, rounds, converged))
}

fn cell(table: &[Vec<f64>], row: usize, column: usize) -> f64 {
    table
        .get(row)
        .and_then(|r| r.get(column))
        .copied()
        .unwrap_or(0.0)
}

fn cell1(table: &[f64], row: usize) -> f64 {
    table.get(row).copied().unwrap_or(0.0)
}

fn add(table: &mut [Vec<f64>], row: usize, column: usize, value: f64) {
    if let Some(slot) = table.get_mut(row).and_then(|r| r.get_mut(column)) {
        *slot += value;
    }
}

fn name(input: &SimInput<'_>, item: usize) -> String {
    input.items.get(item).cloned().unwrap_or_default()
}

/// PT: Resultado de uma passada. EN: Result of one pass.
struct Pass {
    /// PT: Máquina × entrada: itens por segundo recebidos. EN: Machine × input: items per second received.
    got: Vec<Vec<f64>>,
    /// PT: Máquina × produto: itens por segundo retirados. EN: Machine × product: items per second removed.
    removed: Vec<Vec<f64>>,
    moved: Vec<f64>,
    flow: Vec<f64>,
    drained: Vec<f64>,
    /// PT: Quanto os insersores de cada faixa pediram de cada item.
    /// EN: How much each lane's inserters asked for of each item.
    demand_at: Vec<f64>,
}

struct Context<'a, 'b> {
    input: &'b SimInput<'a>,
    items: usize,
    pickers: BTreeMap<usize, Vec<usize>>,
    droppers: BTreeMap<usize, Vec<usize>>,
    source_at: BTreeMap<usize, usize>,
}

impl<'a, 'b> Context<'a, 'b> {
    fn new(input: &'b SimInput<'a>, items: usize) -> Self {
        let mut pickers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut droppers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, inserter) in input.inserters.iter().enumerate() {
            if let End::Belt { tile, .. } = inserter.pickup {
                pickers.entry(tile).or_default().push(k);
            }
            if let End::Belt { tile, .. } = inserter.drop {
                droppers.entry(tile).or_default().push(k);
            }
        }
        let source_at = input
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.tile, i))
            .collect();
        Self {
            input,
            items,
            pickers,
            droppers,
            source_at,
        }
    }

    // PT: O que cada faixa aceita: o que é pedido nela e mais adiante; saída aceita tudo.
    // EN: What each lane accepts: what is requested on it and further on; an output accepts everything.
    fn absorb(&self, demand_at: &[f64]) -> Vec<f64> {
        let belts = self.input.belts;
        let items = self.items;
        let mut absorb = demand_at.to_vec();
        for &sink in &self.input.sinks {
            for lane in 0..2 {
                for item in 0..items {
                    if let Some(a) = absorb.get_mut((sink * 2 + lane) * items + item) {
                        *a = f64::INFINITY;
                    }
                }
            }
        }
        for &tile in self.input.order.iter().rev() {
            let Some(belt) = belts.tiles.get(tile) else {
                continue;
            };
            if self.input.sinks.contains(&tile) {
                continue;
            }
            let targets = belts.successors(tile);
            for lane in 0..2 {
                let target_lane = match belt.output {
                    Output::Sideload { lane, .. } => lane,
                    _ => lane,
                };
                for item in 0..items {
                    let downstream: f64 = targets
                        .iter()
                        .map(|t| {
                            absorb
                                .get((t * 2 + target_lane) * items + item)
                                .copied()
                                .unwrap_or(0.0)
                        })
                        .sum();
                    if let Some(a) = absorb.get_mut((tile * 2 + lane) * items + item) {
                        *a += downstream;
                    }
                }
            }
        }
        absorb
    }

    // PT: Uma passada: máquinas pedem no ritmo `demand` e oferecem no ritmo `supply`.
    // EN: One pass: machines ask at pace `demand` and offer at pace `supply`.
    fn pass(&self, demand: &[f64], supply: &[f64], absorb: &[f64]) -> Pass {
        let input = self.input;
        let machines = &input.machines;
        let items = self.items;
        let mut need: Vec<Vec<f64>> = machines
            .iter()
            .enumerate()
            .map(|(m, machine)| {
                machine
                    .inputs
                    .iter()
                    .map(|(_, per)| cell1(demand, m) * per)
                    .collect()
            })
            .collect();
        let mut got: Vec<Vec<f64>> = machines.iter().map(|m| vec![0.0; m.inputs.len()]).collect();
        let mut available: Vec<Vec<f64>> = machines
            .iter()
            .enumerate()
            .map(|(m, machine)| {
                machine
                    .outputs
                    .iter()
                    .map(|(_, per)| cell1(supply, m) * per)
                    .collect()
            })
            .collect();
        let mut removed: Vec<Vec<f64>> = machines
            .iter()
            .map(|m| vec![0.0; m.outputs.len()])
            .collect();
        let mut capacity: Vec<f64> = input.inserters.iter().map(|i| i.capacity).collect();
        let mut moved = vec![0.0; input.inserters.len()];

        // PT: Insersores sem esteira. EN: Inserters without a belt.
        for (k, inserter) in input.inserters.iter().enumerate() {
            let cap = cell1(&capacity, k);
            let mut used = 0.0;
            match (inserter.pickup, inserter.drop) {
                (End::Storage, End::Machine(m)) => {
                    let count = need.get(m).map_or(0, Vec::len);
                    for slot in 0..count {
                        let take = cell(&need, m, slot).min(cap - used).max(0.0);
                        used += take;
                        add(&mut need, m, slot, -take);
                        add(&mut got, m, slot, take);
                    }
                }
                (End::Machine(p), End::Machine(m)) => {
                    let (Some(source), Some(target)) = (machines.get(p), machines.get(m)) else {
                        continue;
                    };
                    for (out_slot, (item, _)) in source.outputs.iter().enumerate() {
                        let Some(in_slot) = target.inputs.iter().position(|(i, _)| i == item)
                        else {
                            continue;
                        };
                        let take = cell(&available, p, out_slot)
                            .min(cell(&need, m, in_slot))
                            .min(cap - used)
                            .max(0.0);
                        used += take;
                        add(&mut available, p, out_slot, -take);
                        add(&mut removed, p, out_slot, take);
                        add(&mut need, m, in_slot, -take);
                        add(&mut got, m, in_slot, take);
                    }
                }
                (End::Machine(p), End::Storage) => {
                    let count = available.get(p).map_or(0, Vec::len);
                    for slot in 0..count {
                        let take = cell(&available, p, slot).min(cap - used).max(0.0);
                        used += take;
                        add(&mut available, p, slot, -take);
                        add(&mut removed, p, slot, take);
                    }
                }
                _ => continue,
            }
            if let Some(slot) = capacity.get_mut(k) {
                *slot -= used;
            }
            if let Some(slot) = moved.get_mut(k) {
                *slot += used;
            }
        }

        let belts = input.belts;
        let mut flow = vec![0.0; belts.tiles.len() * 2 * items];
        let mut side_in = flow.clone();
        let mut demand_at = flow.clone();
        let mut drained = vec![0.0; items];
        let mut splitter_seen: BTreeMap<usize, usize> = BTreeMap::new();
        let lane_total = |flow: &[f64], node: usize| -> f64 {
            (0..items)
                .map(|i| flow.get(node * items + i).copied().unwrap_or(0.0))
                .sum()
        };
        for &tile in &input.order {
            let Some(belt) = belts.tiles.get(tile) else {
                continue;
            };
            let lane_capacity = belt.lane_capacity();
            for lane in 0..2 {
                let node = tile * 2 + lane;
                // PT: A entrada lateral só ocupa o espaço que a faixa principal deixou.
                // EN: The sideload only takes the room the main lane left.
                let main = lane_total(&flow, node);
                let side = lane_total(&side_in, node);
                let room = (lane_capacity - main).max(0.0);
                let scale = if side > room {
                    room / side.max(1e-12)
                } else {
                    1.0
                };
                for i in 0..items {
                    let c = node * items + i;
                    let extra = side_in.get(c).copied().unwrap_or(0.0) * scale;
                    if let Some(f) = flow.get_mut(c) {
                        *f += extra;
                    }
                }
                let total = lane_total(&flow, node);
                if total > lane_capacity {
                    let scale = lane_capacity / total;
                    for i in 0..items {
                        if let Some(f) = flow.get_mut(node * items + i) {
                            *f *= scale;
                        }
                    }
                }
                // PT: Entrada de esteira: o que a faixa consegue entregar adiante, até a capacidade.
                // EN: Belt input: what the lane can deliver further on, up to the capacity.
                if let Some(source) = self
                    .source_at
                    .get(&tile)
                    .and_then(|i| input.sources.get(*i))
                {
                    let wanted = source.items.get(lane).cloned().unwrap_or_default();
                    let candidates: Vec<usize> = if wanted.is_empty() {
                        (0..items).collect()
                    } else {
                        wanted
                    };
                    let mut room = (lane_capacity - lane_total(&flow, node)).max(0.0);
                    let asks: Vec<f64> = candidates
                        .iter()
                        .map(|&i| {
                            absorb
                                .get(node * items + i)
                                .copied()
                                .unwrap_or(0.0)
                                .min(lane_capacity)
                        })
                        .collect();
                    let total_ask: f64 = asks.iter().sum();
                    let factor = if total_ask > room {
                        room / total_ask.max(1e-12)
                    } else {
                        1.0
                    };
                    for (&i, ask) in candidates.iter().zip(asks) {
                        let share = (ask * factor).min(room).max(0.0);
                        if let Some(f) = flow.get_mut(node * items + i) {
                            *f += share;
                        }
                        room -= share;
                    }
                }
            }

            // PT: Insersores que soltam aqui (saída de máquinas). EN: Inserters dropping here (machine output).
            for &k in self.droppers.get(&tile).map_or(&[][..], Vec::as_slice) {
                let Some(inserter) = input.inserters.get(k) else {
                    continue;
                };
                let (
                    End::Belt {
                        lane: Some(lane), ..
                    },
                    End::Machine(p),
                ) = (inserter.drop, inserter.pickup)
                else {
                    continue;
                };
                let Some(source) = machines.get(p) else {
                    continue;
                };
                let node = tile * 2 + lane;
                let cap = cell1(&capacity, k);
                let mut used = 0.0;
                for (slot, (item, _)) in source.outputs.iter().enumerate() {
                    let c = node * items + item;
                    let space = (lane_capacity - lane_total(&flow, node)).max(0.0);
                    let allowance = (absorb.get(c).copied().unwrap_or(0.0)
                        - flow.get(c).copied().unwrap_or(0.0))
                    .max(0.0);
                    let put = cell(&available, p, slot)
                        .min(cap - used)
                        .min(space)
                        .min(allowance)
                        .max(0.0);
                    used += put;
                    if let Some(f) = flow.get_mut(c) {
                        *f += put;
                    }
                    add(&mut available, p, slot, -put);
                    add(&mut removed, p, slot, put);
                }
                if let Some(slot) = capacity.get_mut(k) {
                    *slot -= used;
                }
                if let Some(slot) = moved.get_mut(k) {
                    *slot += used;
                }
            }

            // PT: Insersores que pegam aqui, das duas faixas. EN: Inserters picking here, from both lanes.
            for &k in self.pickers.get(&tile).map_or(&[][..], Vec::as_slice) {
                let Some(inserter) = input.inserters.get(k) else {
                    continue;
                };
                let cap = cell1(&capacity, k);
                let mut used = 0.0;
                match inserter.drop {
                    End::Machine(m) => {
                        let Some(target) = machines.get(m) else {
                            continue;
                        };
                        for (slot, (item, _)) in target.inputs.iter().enumerate() {
                            let wanted = cell(&need, m, slot);
                            let mut taken = 0.0;
                            for lane in 0..2 {
                                let c = (tile * 2 + lane) * items + item;
                                if let Some(d) = demand_at.get_mut(c) {
                                    *d += (wanted - taken).min(cap - used).max(0.0);
                                }
                                let take = flow
                                    .get(c)
                                    .copied()
                                    .unwrap_or(0.0)
                                    .min(wanted - taken)
                                    .min(cap - used)
                                    .max(0.0);
                                if let Some(f) = flow.get_mut(c) {
                                    *f -= take;
                                }
                                taken += take;
                                used += take;
                            }
                            add(&mut need, m, slot, -taken);
                            add(&mut got, m, slot, taken);
                        }
                    }
                    End::Storage => {
                        for lane in 0..2 {
                            for item in 0..items {
                                let c = (tile * 2 + lane) * items + item;
                                if let Some(d) = demand_at.get_mut(c) {
                                    *d += (cap - used).max(0.0);
                                }
                                let take =
                                    flow.get(c).copied().unwrap_or(0.0).min(cap - used).max(0.0);
                                if let Some(f) = flow.get_mut(c) {
                                    *f -= take;
                                }
                                used += take;
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(slot) = capacity.get_mut(k) {
                    *slot -= used;
                }
                if let Some(slot) = moved.get_mut(k) {
                    *slot += used;
                }
            }

            // PT: O que sobra segue adiante (ou sai, na saída). EN: What is left moves on (or leaves, at an output).
            if input.sinks.contains(&tile) {
                for lane in 0..2 {
                    for item in 0..items {
                        let value = flow
                            .get((tile * 2 + lane) * items + item)
                            .copied()
                            .unwrap_or(0.0);
                        if let Some(d) = drained.get_mut(item) {
                            *d += value;
                        }
                    }
                }
                continue;
            }
            match belt.kind {
                TileKind::Splitter { splitter, .. } => {
                    let seen = splitter_seen.entry(splitter).or_insert(0);
                    *seen += 1;
                    if *seen == 2 {
                        split(
                            belts,
                            splitter,
                            &input.items,
                            items,
                            &mut flow,
                            &mut side_in,
                        );
                    }
                }
                _ => forward(belt.output, tile, items, &mut flow, &mut side_in),
            }
        }
        Pass {
            got,
            removed,
            moved,
            flow,
            drained,
            demand_at,
        }
    }

    fn report(
        &self,
        rate: &[f64],
        limits: &[Limit],
        pass: &Pass,
        rounds: usize,
        converged: bool,
    ) -> FlowReport {
        let input = self.input;
        let machines = input
            .machines
            .iter()
            .zip(rate)
            .zip(limits)
            .map(|((m, rate), limit)| MachineFlow {
                entity_number: m.entity_number,
                crafts_per_second: *rate,
                utilization: if m.max_crafts > 0.0 {
                    rate / m.max_crafts
                } else {
                    0.0
                },
                limit: if m.max_crafts <= 0.0 {
                    Limit::Idle
                } else {
                    limit.clone()
                },
            })
            .collect();
        let inserters = input
            .inserters
            .iter()
            .zip(&pass.moved)
            .map(|(i, moved)| InserterFlow {
                entity_number: i.entity_number,
                items_per_second: *moved,
                capacity: i.capacity,
                utilization: if i.capacity > 0.0 {
                    moved / i.capacity
                } else {
                    0.0
                },
            })
            .collect();
        let mut lanes = Vec::new();
        for (tile, belt) in input.belts.tiles.iter().enumerate() {
            for lane in 0..2 {
                let node = tile * 2 + lane;
                let total: f64 = (0..self.items)
                    .map(|i| pass.flow.get(node * self.items + i).copied().unwrap_or(0.0))
                    .sum();
                lanes.push(LaneFlow {
                    tile,
                    lane,
                    items_per_second: total,
                    capacity: belt.lane_capacity(),
                });
            }
        }
        let mut produced: BTreeMap<String, f64> = BTreeMap::new();
        for (m, r) in input.machines.iter().zip(rate) {
            for (item, per) in &m.outputs {
                *produced.entry(name(input, *item)).or_insert(0.0) += r * per;
            }
        }
        let drained = input
            .items
            .iter()
            .zip(&pass.drained)
            .filter(|(_, r)| **r > 1e-9)
            .map(|(n, r)| (n.clone(), *r))
            .collect();
        FlowReport {
            machines,
            inserters,
            lanes,
            produced,
            drained,
            rounds,
            converged,
        }
    }
}

// PT: Leva a vazão do tile para o próximo: reta e subterrânea mantêm a faixa; entrada lateral
//     junta as duas faixas na faixa do lado (e só ocupa o espaço que sobrar lá).
// EN: Carries the tile's throughput to the next one: straight and underground keep the lane; a
//     sideload joins both lanes onto the side lane (and only takes the room left there).
fn forward(output: Output, tile: usize, items: usize, flow: &mut [f64], side_in: &mut [f64]) {
    match output {
        Output::Straight { tile: target } | Output::Underground { tile: target } => {
            for lane in 0..2 {
                for item in 0..items {
                    let value = flow
                        .get((tile * 2 + lane) * items + item)
                        .copied()
                        .unwrap_or(0.0);
                    if let Some(cell) = flow.get_mut((target * 2 + lane) * items + item) {
                        *cell += value;
                    }
                }
            }
        }
        Output::Sideload {
            tile: target,
            lane: target_lane,
        } => {
            for lane in 0..2 {
                for item in 0..items {
                    let value = flow
                        .get((tile * 2 + lane) * items + item)
                        .copied()
                        .unwrap_or(0.0);
                    if let Some(cell) = side_in.get_mut((target * 2 + target_lane) * items + item) {
                        *cell += value;
                    }
                }
            }
        }
        Output::None => {}
    }
}

// PT: Splitter: junta o que entrou nas duas metades, faixa por faixa, e divide entre as duas
//     saídas: meio a meio, ou primeiro o lado de prioridade (o filtro manda o item filtrado para
//     esse lado e o resto para o outro). Saída inexistente recebe nada.
// EN: Splitter: joins what entered both halves, lane by lane, and divides it between both
//     outputs: half and half, or the priority side first (the filter sends the filtered item to
//     that side and the rest to the other). A missing output receives nothing.
fn split(
    belts: &BeltNetwork,
    splitter: usize,
    names: &[String],
    items: usize,
    flow: &mut [f64],
    side_in: &mut [f64],
) {
    let Some(s) = belts.splitters.get(splitter) else {
        return;
    };
    let [left, right] = s.halves;
    let open = |half: usize| {
        belts
            .tiles
            .get(half)
            .is_some_and(|t| t.output != Output::None)
    };
    let capacity = belts.tiles.get(left).map_or(0.0, |t| t.lane_capacity());
    let filter = s
        .filter
        .as_ref()
        .and_then(|f| names.iter().position(|n| n == f));
    for lane in 0..2 {
        let totals: Vec<f64> = (0..items)
            .map(|item| {
                [left, right]
                    .iter()
                    .map(|h| {
                        flow.get((h * 2 + lane) * items + item)
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .sum()
            })
            .collect();
        // PT: Com prioridade, o lado preferido leva até a capacidade da faixa (somando todos os
        //     itens, na proporção de cada um) e o resto vai para o outro lado.
        // EN: With priority, the preferred side takes up to the lane capacity (summing every item,
        //     in each one's proportion) and the rest goes to the other side.
        let lane_total: f64 = totals.iter().sum();
        let share = if lane_total > capacity {
            capacity / lane_total
        } else {
            1.0
        };
        for (item, total) in totals.iter().copied().enumerate() {
            let (to_left, to_right) = match (open(left), open(right), filter, s.output_priority) {
                (true, false, _, _) => (total, 0.0),
                (false, true, _, _) => (0.0, total),
                (false, false, _, _) => (0.0, 0.0),
                // PT: Filtro: o item filtrado só vai para o lado de prioridade; o resto, só para o outro.
                // EN: Filter: the filtered item only goes to the priority side; the rest, only to the other.
                (true, true, Some(f), priority) => {
                    let to_priority = f == item;
                    let left_side = (priority.unwrap_or(Side::Left) == Side::Left) == to_priority;
                    if left_side {
                        (total, 0.0)
                    } else {
                        (0.0, total)
                    }
                }
                (true, true, None, Some(Side::Left)) => (total * share, total * (1.0 - share)),
                (true, true, None, Some(Side::Right)) => (total * (1.0 - share), total * share),
                (true, true, None, None) => (total / 2.0, total / 2.0),
            };
            if let Some(cell) = flow.get_mut((left * 2 + lane) * items + item) {
                *cell = to_left;
            }
            if let Some(cell) = flow.get_mut((right * 2 + lane) * items + item) {
                *cell = to_right;
            }
        }
    }
    for half in [left, right] {
        let output = belts.tiles.get(half).map_or(Output::None, |t| t.output);
        forward(output, half, items, flow, side_in);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bf_world::belts::{BeltTile, Splitter, TileKind};
    use bf_world::dir::Dir;

    fn tile(x: i64, kind: TileKind, output: Output) -> BeltTile {
        BeltTile {
            entity: 0,
            x,
            y: 0,
            dir: Dir::East,
            kind,
            speed: 0.03125,
            output,
            curve: None,
        }
    }

    // PT: Faixa mista (10 + 10 por segundo) num splitter amarelo com prioridade à esquerda: o lado
    //     preferido leva 7,5 (a capacidade da faixa) e o resto, 12,5, vai para o outro.
    // EN: Mixed lane (10 + 10 per second) into a yellow splitter with left priority: the preferred
    //     side takes 7.5 (the lane capacity) and the rest, 12.5, goes to the other one.
    #[test]
    fn priority_caps_the_whole_lane() {
        let mut belts = BeltNetwork::default();
        belts.tiles = vec![
            tile(
                0,
                TileKind::Splitter {
                    splitter: 0,
                    half: 0,
                },
                Output::Straight { tile: 2 },
            ),
            tile(
                0,
                TileKind::Splitter {
                    splitter: 0,
                    half: 1,
                },
                Output::Straight { tile: 3 },
            ),
            tile(1, TileKind::Belt, Output::None),
            tile(1, TileKind::Belt, Output::None),
        ];
        belts.splitters = vec![Splitter {
            entity: 0,
            halves: [0, 1],
            input_priority: None,
            output_priority: Some(Side::Left),
            filter: None,
        }];
        let items = 2;
        let mut flow = vec![0.0; belts.tiles.len() * 2 * items];
        let mut side_in = flow.clone();
        flow[0] = 10.0;
        flow[1] = 10.0;
        split(
            &belts,
            0,
            &["a".into(), "b".into()],
            items,
            &mut flow,
            &mut side_in,
        );
        let lane0 = |t: usize| flow[t * 2 * items] + flow[t * 2 * items + 1];
        assert!((lane0(2) - 7.5).abs() < 1e-9, "{}", lane0(2));
        assert!((lane0(3) - 12.5).abs() < 1e-9, "{}", lane0(3));
    }
}
