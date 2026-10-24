//! PT: Simulação tick a tick. Cada faixa de cada tile guarda os itens em posições de 1/256 de
//!     tile, a 0,25 tile (64) um do outro, andando `velocidade × 256` por tick; as faixas são
//!     processadas de quem recebe para quem manda, para o espaço liberado andar no mesmo tick.
//!     Cada insersor é uma máquina de estados (pegar, girar até a entrega, entregar, voltar) com
//!     a meia-volta medida no jogo; na esteira ele pega um item por tick dentro do alcance e, sem
//!     mão cheia, espera `patience_ticks` antes de sair. As máquinas aceitam insumos até cerca de
//!     dois ciclos e só começam outro ciclo com no máximo uma receita pronta na saída. Tudo é
//!     medido numa janela depois do aquecimento.
//! EN: Tick-by-tick simulation. Each lane of each tile keeps its items at 1/256-tile positions,
//!     0.25 tile (64) apart, moving `speed × 256` per tick; lanes are processed from receivers to
//!     senders, so freed space moves within the same tick. Each inserter is a state machine (pick,
//!     swing to the drop, drop, swing back) with the half swing measured in-game; on a belt it
//!     picks one item per tick within reach and, without a full hand, waits `patience_ticks`
//!     before leaving. Machines accept inputs up to about two cycles and only start another cycle
//!     with at most one recipe ready in the output. Everything is measured in a window after the
//!     warm-up.

use std::collections::BTreeMap;

use bf_world::belts::{Output, Side, TileKind};

use crate::model::{End, SimInput};
use crate::{FlowReport, InserterFlow, LaneFlow, Limit, MachineFlow, SimError};

/// PT: Comprimento de um tile em unidades de posição. EN: Length of one tile in position units.
pub const TILE: i32 = 256;
/// PT: Distância mínima entre dois itens na mesma faixa. EN: Minimum distance between two items on one lane.
pub const GAP: i32 = 64;
const MIDDLE: i32 = 128;
/// PT: Teto de trabalho (entidades × ticks). EN: Work cap (entities × ticks).
pub const MAX_WORK: u64 = 3_000_000_000;

/// PT: Parâmetros da simulação por tick. EN: Tick simulation parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickOptions {
    pub warmup_ticks: u32,
    pub measure_ticks: u32,
    /// PT: Alcance do insersor ao longo da faixa, a partir do centro do tile.
    /// EN: Inserter reach along the lane, from the tile center.
    pub pickup_reach: i32,
    /// PT: Ticks que o insersor espera sem pegar nada antes de sair com a mão incompleta.
    /// EN: Ticks the inserter waits without picking anything before leaving with a partial hand.
    pub patience_ticks: u32,
    /// PT: Espaço livre que o insersor precisa na faixa para soltar um item.
    /// EN: Free room the inserter needs on the lane to drop one item.
    pub drop_gap: i32,
    /// PT: Item que sai de um baú para uma esteira (sem ele, baú → esteira não move nada).
    /// EN: Item that leaves a chest onto a belt (without it, chest → belt moves nothing).
    pub storage_item: Option<usize>,
}

impl Default for TickOptions {
    fn default() -> Self {
        Self {
            warmup_ticks: 36_000,
            measure_ticks: 7_200,
            // PT: Calibrados por `bf-validate calibrate-tick` (erro médio de 3,4% na tabela medida).
            // EN: Calibrated by `bf-validate calibrate-tick` (3.4% mean error on the measured table).
            pickup_reach: 16,
            patience_ticks: 8,
            drop_gap: 32,
            storage_item: None,
        }
    }
}

/// PT: Roda com as opções padrão. EN: Runs with the default options.
pub fn simulate(input: &SimInput<'_>) -> Result<FlowReport, SimError> {
    simulate_with(input, &TickOptions::default(), &mut |_| true)
}

/// PT: Roda com opções e um retorno de progresso (0..1) que pode cancelar devolvendo `false`.
/// EN: Runs with options and a progress callback (0..1) that can cancel by returning `false`.
pub fn simulate_with(
    input: &SimInput<'_>,
    options: &TickOptions,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<FlowReport, SimError> {
    let total = u64::from(options.warmup_ticks) + u64::from(options.measure_ticks);
    let size = (input.belts.tiles.len() + input.inserters.len() + input.machines.len()) as u64;
    if size.saturating_mul(total) > MAX_WORK {
        return Err(SimError::TooLarge);
    }
    let mut world = TickWorld::new(input, options);
    for tick in 0..total {
        world.measuring = tick >= u64::from(options.warmup_ticks);
        world.step(input);
        if tick % 600 == 0 && !progress(tick as f64 / total as f64) {
            return Err(SimError::Cancelled);
        }
    }
    Ok(world.report(input, options))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pickup,
    ToDrop(u32),
    Drop,
    ToPickup(u32),
}

#[derive(Debug, Clone)]
struct Arm {
    phase: Phase,
    item: usize,
    count: u32,
    idle: u32,
}

#[derive(Debug, Clone)]
struct Crafter {
    crafting: bool,
    progress: f64,
    input: Vec<f64>,
    output: Vec<f64>,
    limit: Vec<f64>,
    per_tick: f64,
}

/// PT: Para onde sai uma faixa. EN: Where a lane exits to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Blocked,
    /// PT: Entra pelo começo da faixa `node`. EN: Enters at the start of lane `node`.
    Back(usize),
    /// PT: Entra no meio da faixa `node` (entrada lateral). EN: Enters the middle of lane `node` (sideload).
    Middle(usize),
    /// PT: Saída de splitter: escolhe entre as duas metades. EN: Splitter exit: picks between both halves.
    Splitter {
        splitter: usize,
        lane: usize,
    },
}

struct TickWorld {
    lanes: Vec<Vec<(i32, usize)>>,
    exits: Vec<Exit>,
    speed: Vec<i32>,
    order: Vec<usize>,
    arms: Vec<Arm>,
    crafters: Vec<Crafter>,
    /// PT: Itens de cada entrada, por tile (None se o tile não é entrada). EN: Items of each input, per tile (None if the tile is not an input).
    source_items: Vec<Option<[Vec<usize>; 2]>>,
    source_turn: Vec<usize>,
    is_sink: Vec<bool>,
    splitter_turn: Vec<[usize; 2]>,
    splitter_targets: Vec<[Exit; 2]>,
    measuring: bool,
    produced: Vec<f64>,
    crafts: Vec<f64>,
    moved: Vec<f64>,
    passed: Vec<f64>,
    drained: Vec<f64>,
    reach: i32,
    patience: u32,
    drop_gap: i32,
    storage_item: Option<usize>,
}

impl TickWorld {
    fn new(input: &SimInput<'_>, options: &TickOptions) -> Self {
        let belts = input.belts;
        let tiles = belts.tiles.len();
        let mut exits = vec![Exit::Blocked; tiles * 2];
        for (index, tile) in belts.tiles.iter().enumerate() {
            for lane in 0..2 {
                let exit = match (tile.kind, tile.output) {
                    (TileKind::Splitter { splitter, .. }, _) => Exit::Splitter { splitter, lane },
                    (_, Output::Straight { tile: t } | Output::Underground { tile: t }) => {
                        Exit::Back(t * 2 + lane)
                    }
                    (_, Output::Sideload { tile: t, lane: l }) => Exit::Middle(t * 2 + l),
                    (_, Output::None) => Exit::Blocked,
                };
                if let Some(slot) = exits.get_mut(index * 2 + lane) {
                    *slot = exit;
                }
            }
        }
        // PT: Saída de cada metade de splitter: reta (pelo começo da faixa) ou lateral (pelo meio
        //     da faixa do lado); o nó final depende da faixa do item.
        // EN: Exit of each splitter half: straight (at the lane start) or sideways (at the middle
        //     of the side lane); the final node depends on the item's lane.
        let splitter_targets = belts
            .splitters
            .iter()
            .map(|s| {
                s.halves
                    .map(|h| match belts.tiles.get(h).map(|t| t.output) {
                        Some(Output::Straight { tile } | Output::Underground { tile }) => {
                            Exit::Back(tile)
                        }
                        Some(Output::Sideload { tile, lane }) => Exit::Middle(tile * 2 + lane),
                        _ => Exit::Blocked,
                    })
            })
            .collect();
        let mut source_items = vec![None; tiles];
        for source in &input.sources {
            if let Some(slot) = source_items.get_mut(source.tile) {
                *slot = Some(source.items.clone());
            }
        }
        let mut is_sink = vec![false; tiles];
        for &sink in &input.sinks {
            if let Some(slot) = is_sink.get_mut(sink) {
                *slot = true;
            }
        }
        let crafters = input
            .machines
            .iter()
            .map(|m| {
                let cps = m.max_crafts;
                let cycles = 2.0_f64.max((cps * 1.166).ceil() + 1.0);
                Crafter {
                    crafting: false,
                    progress: 0.0,
                    input: vec![0.0; m.inputs.len()],
                    output: vec![0.0; m.outputs.len()],
                    limit: m.inputs.iter().map(|(_, per)| per * cycles).collect(),
                    per_tick: cps / 60.0,
                }
            })
            .collect();
        let mut order = input.order.clone();
        order.reverse();
        Self {
            lanes: vec![Vec::new(); tiles * 2],
            exits,
            speed: belts
                .tiles
                .iter()
                .map(|t| (t.speed * f64::from(TILE)).round() as i32)
                .collect(),
            order,
            arms: input
                .inserters
                .iter()
                .map(|_| Arm {
                    phase: Phase::Pickup,
                    item: 0,
                    count: 0,
                    idle: 0,
                })
                .collect(),
            crafters,
            source_items,
            source_turn: vec![0; tiles * 2],
            is_sink,
            splitter_turn: vec![[0; 2]; belts.splitters.len()],
            splitter_targets,
            measuring: false,
            produced: vec![0.0; input.items.len()],
            crafts: vec![0.0; input.machines.len()],
            moved: vec![0.0; input.inserters.len()],
            passed: vec![0.0; tiles * 2],
            drained: vec![0.0; input.items.len()],
            reach: options.pickup_reach,
            patience: options.patience_ticks,
            drop_gap: options.drop_gap,
            storage_item: options.storage_item,
        }
    }

    fn step(&mut self, input: &SimInput<'_>) {
        self.belts(input);
        self.inserters(input);
        self.machines(input);
    }

    // PT: Posição do último item (o mais perto do começo) de uma faixa. EN: Position of a lane's last item (closest to the start).
    fn back(&self, node: usize) -> Option<i32> {
        self.lanes.get(node).and_then(|l| l.last()).map(|(p, _)| *p)
    }

    fn fits_middle(&self, node: usize, at: i32) -> bool {
        self.lanes
            .get(node)
            .is_none_or(|lane| lane.iter().all(|(p, _)| (p - at).abs() >= GAP))
    }

    fn insert_middle(&mut self, node: usize, at: i32, item: usize) {
        if let Some(lane) = self.lanes.get_mut(node) {
            let index = lane.iter().position(|(p, _)| *p < at).unwrap_or(lane.len());
            lane.insert(index, (at, item));
        }
    }

    fn belts(&mut self, input: &SimInput<'_>) {
        let order = std::mem::take(&mut self.order);
        for &tile in &order {
            for lane in 0..2 {
                let node = tile * 2 + lane;
                let speed = self.speed.get(tile).copied().unwrap_or(0);
                let exit = self.exits.get(node).copied().unwrap_or(Exit::Blocked);
                let front_limit = match exit {
                    Exit::Back(next) => self.back(next).map_or(i32::MAX, |b| TILE + b - GAP),
                    Exit::Middle(next) => {
                        if self.fits_middle(next, MIDDLE) {
                            i32::MAX
                        } else {
                            TILE - 1
                        }
                    }
                    Exit::Splitter { .. } => i32::MAX,
                    Exit::Blocked => TILE - 1,
                };
                let Some(items) = self.lanes.get_mut(node) else {
                    continue;
                };
                let mut ahead = front_limit;
                for entry in items.iter_mut() {
                    let target = (entry.0 + speed).min(ahead);
                    if target > entry.0 {
                        entry.0 = target;
                    }
                    ahead = entry.0 - GAP;
                }
                // PT: Itens que passaram do fim do tile seguem adiante. EN: Items past the tile's end move on.
                while let Some(&(position, item)) = self.lanes.get(node).and_then(|l| l.first()) {
                    if position < TILE {
                        break;
                    }
                    let moved = match exit {
                        Exit::Back(next) => {
                            if let Some(lane) = self.lanes.get_mut(next) {
                                lane.push((position - TILE, item));
                            }
                            true
                        }
                        Exit::Middle(next) => {
                            if self.fits_middle(next, MIDDLE) {
                                self.insert_middle(next, MIDDLE, item);
                                true
                            } else {
                                false
                            }
                        }
                        Exit::Splitter { splitter, lane } => {
                            self.split(input, splitter, lane, position - TILE, item)
                        }
                        Exit::Blocked => false,
                    };
                    if let Some(l) = self.lanes.get_mut(node) {
                        if moved {
                            l.remove(0);
                        } else if let Some(first) = l.first_mut() {
                            first.0 = TILE - 1;
                        }
                    }
                    if !moved {
                        break;
                    }
                    if self.measuring
                        && let Some(p) = self.passed.get_mut(node)
                    {
                        *p += 1.0;
                    }
                }
            }
            // PT: Entrada: põe um item no começo de cada faixa que tiver espaço.
            // EN: Input: puts an item at the start of each lane that has room.
            for lane in 0..2 {
                let node = tile * 2 + lane;
                let item = match self
                    .source_items
                    .get(tile)
                    .and_then(Option::as_ref)
                    .and_then(|l| l.get(lane))
                {
                    Some(wanted)
                        if !wanted.is_empty() && self.back(node).is_none_or(|b| b >= GAP) =>
                    {
                        let turn = self.source_turn.get(node).copied().unwrap_or(0);
                        wanted.get(turn % wanted.len()).copied()
                    }
                    _ => None,
                };
                if let Some(item) = item {
                    if let Some(turn) = self.source_turn.get_mut(node) {
                        *turn += 1;
                    }
                    if let Some(l) = self.lanes.get_mut(node) {
                        l.push((0, item));
                    }
                }
            }
            // PT: Saída: tudo o que chega some (e é contado). EN: Output: everything arriving disappears (and is counted).
            if self.is_sink.get(tile).copied().unwrap_or(false) {
                for lane in 0..2 {
                    if let Some(l) = self.lanes.get_mut(tile * 2 + lane) {
                        for (_, item) in l.drain(..) {
                            if self.measuring
                                && let Some(d) = self.drained.get_mut(item)
                            {
                                *d += 1.0;
                            }
                        }
                    }
                }
            }
        }
        self.order = order;
    }

    // PT: Splitter: alterna entre as saídas; prioridade e filtro escolhem o lado primeiro.
    // EN: Splitter: alternates between the outputs; priority and filter pick the side first.
    fn split(
        &mut self,
        input: &SimInput<'_>,
        splitter: usize,
        lane: usize,
        position: i32,
        item: usize,
    ) -> bool {
        let Some(info) = input.belts.splitters.get(splitter) else {
            return false;
        };
        let targets = self
            .splitter_targets
            .get(splitter)
            .copied()
            .unwrap_or([Exit::Blocked, Exit::Blocked]);
        let filtered = info
            .filter
            .as_ref()
            .and_then(|f| input.items.iter().position(|n| n == f));
        let side_index = |side: Side| if side == Side::Left { 0 } else { 1 };
        let choices: Vec<usize> = match (filtered, info.output_priority) {
            (Some(f), priority) => {
                let side = side_index(priority.unwrap_or(Side::Left));
                vec![if f == item { side } else { 1 - side }]
            }
            (None, Some(priority)) => {
                let side = side_index(priority);
                vec![side, 1 - side]
            }
            (None, None) => {
                let turn = self
                    .splitter_turn
                    .get(splitter)
                    .and_then(|t| t.get(lane))
                    .copied()
                    .unwrap_or(0);
                vec![turn % 2, 1 - turn % 2]
            }
        };
        for half in choices {
            let placed = match targets.get(half).copied().unwrap_or(Exit::Blocked) {
                Exit::Back(tile) => {
                    let node = tile * 2 + lane;
                    let fits = self.back(node).is_none_or(|b| b - position >= GAP);
                    if fits && let Some(l) = self.lanes.get_mut(node) {
                        l.push((position, item));
                    }
                    fits
                }
                Exit::Middle(node) => {
                    let fits = self.fits_middle(node, MIDDLE);
                    if fits {
                        self.insert_middle(node, MIDDLE, item);
                    }
                    fits
                }
                _ => false,
            };
            if placed {
                if let Some(turn) = self
                    .splitter_turn
                    .get_mut(splitter)
                    .and_then(|t| t.get_mut(lane))
                {
                    *turn = half + 1;
                }
                return true;
            }
        }
        false
    }

    // PT: O que a máquina `m` aceita agora. EN: What machine `m` accepts now.
    fn accepts(&self, input: &SimInput<'_>, m: usize, item: usize) -> bool {
        let (Some(machine), Some(state)) = (input.machines.get(m), self.crafters.get(m)) else {
            return false;
        };
        machine.inputs.iter().enumerate().any(|(slot, (i, _))| {
            *i == item
                && state
                    .input
                    .get(slot)
                    .zip(state.limit.get(slot))
                    .is_some_and(|(c, l)| c < l)
        })
    }

    fn inserters(&mut self, input: &SimInput<'_>) {
        for (k, inserter) in input.inserters.iter().enumerate() {
            if inserter.capacity <= 0.0 {
                continue;
            }
            let half = inserter.half_swing_ticks;
            let hand = inserter.hand_size;
            let Some(phase) = self.arms.get(k).map(|a| a.phase) else {
                continue;
            };
            match phase {
                Phase::Pickup => {
                    let grabbed = self.pickup(input, k);
                    let Some(arm) = self.arms.get_mut(k) else {
                        continue;
                    };
                    if grabbed {
                        arm.idle = 0;
                    } else if arm.count > 0 {
                        arm.idle += 1;
                    }
                    let leave = arm.count >= hand
                        || (arm.count > 0 && !matches!(inserter.pickup, End::Belt { .. }))
                        || (arm.count > 0 && arm.idle > self.patience);
                    if leave {
                        arm.phase = Phase::ToDrop(half);
                        arm.idle = 0;
                    }
                }
                Phase::ToDrop(left) => {
                    let next = if left <= 1 {
                        Phase::Drop
                    } else {
                        Phase::ToDrop(left - 1)
                    };
                    if let Some(arm) = self.arms.get_mut(k) {
                        arm.phase = next;
                    }
                    if next == Phase::Drop {
                        self.drop(input, k);
                    }
                }
                Phase::Drop => self.drop(input, k),
                Phase::ToPickup(left) => {
                    let next = if left <= 1 {
                        Phase::Pickup
                    } else {
                        Phase::ToPickup(left - 1)
                    };
                    if let Some(arm) = self.arms.get_mut(k) {
                        arm.phase = next;
                    }
                    if next == Phase::Pickup {
                        let grabbed = self.pickup(input, k);
                        if let Some(arm) = self.arms.get_mut(k)
                            && (arm.count >= hand
                                || (grabbed && !matches!(inserter.pickup, End::Belt { .. })))
                        {
                            arm.phase = Phase::ToDrop(half);
                        }
                    }
                }
            }
        }
    }

    // PT: Tenta pegar; devolve se pegou algo neste tick. EN: Tries to pick up; returns whether it picked something this tick.
    fn pickup(&mut self, input: &SimInput<'_>, k: usize) -> bool {
        let Some(inserter) = input.inserters.get(k) else {
            return false;
        };
        let (count, held) = self.arms.get(k).map_or((0, 0), |a| (a.count, a.item));
        let room = inserter.hand_size.saturating_sub(count);
        if room == 0 {
            return false;
        }
        let wanted = |world: &Self, item: usize| -> bool {
            (count == 0 || item == held)
                && match inserter.drop {
                    End::Machine(m) => world.accepts(input, m, item),
                    End::Storage | End::Belt { .. } => true,
                    End::Nothing => false,
                }
        };
        match inserter.pickup {
            End::Storage => {
                // PT: Baú: a mão enche do insumo que o destino mais precisa. EN: Chest: the hand fills with the input the target needs most.
                let End::Machine(m) = inserter.drop else {
                    if let (Some(item), Some(arm)) = (self.storage_item, self.arms.get_mut(k)) {
                        arm.item = item;
                        arm.count = inserter.hand_size;
                        return true;
                    }
                    return false;
                };
                let Some(machine) = input.machines.get(m) else {
                    return false;
                };
                let Some(state) = self.crafters.get(m) else {
                    return false;
                };
                let best = machine
                    .inputs
                    .iter()
                    .enumerate()
                    .filter(|(slot, _)| state.input.get(*slot) < state.limit.get(*slot))
                    .min_by(|(a, _), (b, _)| {
                        let fill = |s: &usize| {
                            state.input.get(*s).copied().unwrap_or(0.0)
                                / state.limit.get(*s).copied().unwrap_or(1.0).max(1e-9)
                        };
                        fill(a).total_cmp(&fill(b))
                    })
                    .map(|(_, (item, _))| *item);
                if let (Some(item), Some(arm)) = (best, self.arms.get_mut(k)) {
                    arm.item = item;
                    arm.count = inserter.hand_size;
                    return true;
                }
                false
            }
            End::Machine(p) => {
                let Some(machine) = input.machines.get(p) else {
                    return false;
                };
                for (slot, (item, _)) in machine.outputs.iter().enumerate() {
                    let have = self
                        .crafters
                        .get(p)
                        .and_then(|c| c.output.get(slot))
                        .copied()
                        .unwrap_or(0.0)
                        .floor();
                    if have >= 1.0 && wanted(self, *item) {
                        let take = (have as u32).min(room);
                        if let Some(out) = self
                            .crafters
                            .get_mut(p)
                            .and_then(|c| c.output.get_mut(slot))
                        {
                            *out -= f64::from(take);
                        }
                        if let Some(arm) = self.arms.get_mut(k) {
                            arm.item = *item;
                            arm.count += take;
                        }
                        return true;
                    }
                }
                false
            }
            End::Belt { tile, .. } => {
                for lane in 0..2 {
                    let node = tile * 2 + lane;
                    let found = self.lanes.get(node).and_then(|l| {
                        l.iter().position(|(p, item)| {
                            (p - MIDDLE).abs() <= self.reach && wanted(self, *item)
                        })
                    });
                    if let Some(index) = found {
                        let Some((_, item)) = self.lanes.get_mut(node).map(|l| l.remove(index))
                        else {
                            continue;
                        };
                        if let Some(arm) = self.arms.get_mut(k) {
                            arm.item = item;
                            arm.count += 1;
                        }
                        return true;
                    }
                }
                false
            }
            End::Nothing => false,
        }
    }

    fn drop(&mut self, input: &SimInput<'_>, k: usize) {
        let Some(inserter) = input.inserters.get(k) else {
            return;
        };
        let Some((item, count)) = self.arms.get(k).map(|a| (a.item, a.count)) else {
            return;
        };
        let mut delivered = 0;
        match inserter.drop {
            End::Machine(m) => {
                if count > 0 && self.accepts(input, m, item) {
                    let slot = input
                        .machines
                        .get(m)
                        .and_then(|machine| machine.inputs.iter().position(|(i, _)| *i == item));
                    if let Some(c) =
                        slot.and_then(|s| self.crafters.get_mut(m).and_then(|c| c.input.get_mut(s)))
                    {
                        *c += f64::from(count);
                        delivered = count;
                    }
                }
            }
            End::Storage => delivered = count,
            End::Belt { tile, lane } => {
                let node = tile * 2 + lane.unwrap_or(1);
                let room = self
                    .lanes
                    .get(node)
                    .is_none_or(|l| l.iter().all(|(p, _)| (p - MIDDLE).abs() >= self.drop_gap));
                if count > 0 && room {
                    self.insert_middle(node, MIDDLE, item);
                    delivered = 1;
                }
            }
            End::Nothing => {}
        }
        if let Some(arm) = self.arms.get_mut(k) {
            arm.count -= delivered;
            if arm.count == 0 {
                let back = if matches!(inserter.pickup, End::Belt { .. }) {
                    (i64::from(inserter.half_swing_ticks) - i64::from(inserter.belt_pickup_saving))
                        .max(1)
                } else {
                    i64::from(inserter.half_swing_ticks)
                };
                arm.phase = Phase::ToPickup(u32::try_from(back).unwrap_or(1));
            }
        }
        if self.measuring
            && let Some(m) = self.moved.get_mut(k)
        {
            *m += f64::from(delivered);
        }
    }

    fn machines(&mut self, input: &SimInput<'_>) {
        for (m, machine) in input.machines.iter().enumerate() {
            let Some(state) = self.crafters.get_mut(m) else {
                continue;
            };
            if state.per_tick <= 0.0 {
                continue;
            }
            let mut budget = state.per_tick;
            loop {
                if !state.crafting {
                    let ready = machine.inputs.iter().enumerate().all(|(slot, (_, per))| {
                        state.input.get(slot).is_some_and(|c| *c + 1e-9 >= *per)
                    });
                    // PT: No jogo a máquina só começa outro ciclo com no máximo uma receita pronta
                    //     na saída (conferido nas saídas das máquinas da blueprint 07).
                    // EN: In the game a machine only starts another cycle with at most one recipe
                    //     ready in its output (checked on the outputs of blueprint 07's machines).
                    let room = machine.outputs.iter().enumerate().all(|(slot, (_, per))| {
                        state.output.get(slot).copied().unwrap_or(0.0) <= per.max(1.0) + 1e-9
                    });
                    if !(ready && room) {
                        break;
                    }
                    for (slot, (_, per)) in machine.inputs.iter().enumerate() {
                        if let Some(c) = state.input.get_mut(slot) {
                            *c -= per;
                        }
                    }
                    state.crafting = true;
                }
                let need = 1.0 - state.progress;
                if budget < need {
                    state.progress += budget;
                    break;
                }
                budget -= need;
                state.progress = 0.0;
                state.crafting = false;
                for (slot, (item, per)) in machine.outputs.iter().enumerate() {
                    if let Some(o) = state.output.get_mut(slot) {
                        *o += per;
                    }
                    if self.measuring
                        && let Some(p) = self.produced.get_mut(*item)
                    {
                        *p += per;
                    }
                }
                if self.measuring
                    && let Some(c) = self.crafts.get_mut(m)
                {
                    *c += 1.0;
                }
            }
        }
    }

    fn report(&self, input: &SimInput<'_>, options: &TickOptions) -> FlowReport {
        let seconds = f64::from(options.measure_ticks.max(1)) / 60.0;
        let machines = input
            .machines
            .iter()
            .zip(&self.crafts)
            .map(|(m, crafts)| {
                let rate = crafts / seconds;
                MachineFlow {
                    entity_number: m.entity_number,
                    crafts_per_second: rate,
                    utilization: if m.max_crafts > 0.0 {
                        rate / m.max_crafts
                    } else {
                        0.0
                    },
                    limit: if m.max_crafts <= 0.0 {
                        Limit::Idle
                    } else {
                        Limit::None
                    },
                }
            })
            .collect();
        let inserters = input
            .inserters
            .iter()
            .zip(&self.moved)
            .map(|(i, moved)| {
                let rate = moved / seconds;
                InserterFlow {
                    entity_number: i.entity_number,
                    items_per_second: rate,
                    capacity: i.capacity,
                    utilization: if i.capacity > 0.0 {
                        rate / i.capacity
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        let lanes = input
            .belts
            .tiles
            .iter()
            .enumerate()
            .flat_map(|(tile, belt)| (0..2).map(move |lane| (tile, lane, belt.lane_capacity())))
            .map(|(tile, lane, capacity)| LaneFlow {
                tile,
                lane,
                items_per_second: self.passed.get(tile * 2 + lane).copied().unwrap_or(0.0)
                    / seconds,
                capacity,
            })
            .collect();
        let named = |values: &[f64]| -> BTreeMap<String, f64> {
            input
                .items
                .iter()
                .zip(values)
                .filter(|(_, v)| **v > 0.0)
                .map(|(n, v)| (n.clone(), v / seconds))
                .collect()
        };
        FlowReport {
            machines,
            inserters,
            lanes,
            produced: named(&self.produced),
            drained: named(&self.drained),
            rounds: (options.warmup_ticks + options.measure_ticks) as usize,
            converged: true,
        }
    }
}
