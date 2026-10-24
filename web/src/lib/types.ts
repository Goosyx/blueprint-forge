// PT: Tipos do resultado da análise, espelhando `bf_analysis::Analysis` em Rust.
//     O teste `wasm.test.ts` confere que o JSON real do WASM segue estes tipos.
// EN: Analysis result types, mirroring `bf_analysis::Analysis` in Rust.
//     The `wasm.test.ts` test checks that the real WASM JSON follows these types.

export interface IconRef {
  kind: string;
  name: string;
}

export type TreeNode =
  | { type: 'book'; label: string | null; icons: IconRef[]; children: TreeNode[] }
  | { type: 'blueprint'; blueprint: number }
  | { type: 'other'; kind: string };

export interface Footprint {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PlacedEntity {
  index: number;
  entity_number: number;
  name: string;
  prototype: string | null;
  direction: number;
  footprint: Footprint;
  recipe?: string;
}

export type WorldWarning =
  | { code: 'unknown_entity'; entity_number: number; name: string }
  | { code: 'overlap'; first: number; second: number; x: number; y: number };

export interface Bounds {
  min_x: number;
  min_y: number;
  max_x: number;
  max_y: number;
}

export interface World {
  entities: PlacedEntity[];
  bounds?: Bounds;
  warnings: WorldWarning[];
  dropped_warnings: number;
}

export interface BlueprintSummary {
  label: string | null;
  description: string | null;
  icons: IconRef[];
  entity_count: number;
}

export interface Effect {
  speed: number;
  productivity: number;
  consumption: number;
  pollution: number;
  quality: number;
}

export interface MachineReport {
  entity_number: number;
  name: string;
  recipe?: string;
  recipe_source?: 'blueprint' | 'override' | 'fixed' | 'inferred';
  modules?: Record<string, number>;
  beacons: number;
  effect: Effect;
  crafts_per_second: number;
  power_w: number;
  drain_w: number;
  electric: boolean;
  powered: boolean;
  pollution_per_minute: number;
}

export interface RecipeGroup {
  recipe: string;
  machines: number;
  produces: Record<string, number>;
  consumes: Record<string, number>;
  power_w: number;
  pollution_per_minute: number;
}

export interface ItemFlow {
  name: string;
  kind: 'item' | 'fluid';
  produced_per_second: number;
  consumed_per_second: number;
}

export interface SteadyState {
  utilization: Record<string, number>;
  bottlenecks: string[];
  solved: boolean;
}

export interface PowerReport {
  average_w: number;
  peak_w: number;
  drain_w: number;
  by_category: Record<string, number>;
}

export interface GenerationReport {
  electric_capacity_w: number;
  steam_electric_w: number;
  solar_peak_w: number;
  solar_average_w: number;
  accumulator_energy_j: number;
  water_per_second: number;
  steam_per_second: Record<string, number>;
  reactor_heat_w: number;
  boiler_fuel_per_second: number;
}

export interface BomLine {
  item: string;
  count: number;
}

export type CalcWarning =
  | { code: 'no_recipe'; entity_number: number }
  | { code: 'unknown_recipe'; entity_number: number; recipe: string }
  | { code: 'recipe_not_allowed'; entity_number: number; recipe: string }
  | { code: 'unpowered'; entity_number: number };

export interface CalcReport {
  machines: MachineReport[];
  groups: RecipeGroup[];
  nominal: ItemFlow[];
  steady: ItemFlow[];
  steady_state: SteadyState;
  power: PowerReport;
  generation: GenerationReport;
  pollution_per_minute: number;
  bill_of_materials: BomLine[];
  inferences: Inference[];
  warnings: CalcWarning[];
}

export interface Inference {
  entity_number: number;
  recipe: string;
  confidence: 'high' | 'low';
  alternatives?: string[];
}

export type Limit =
  | { type: 'none' }
  | { type: 'idle' }
  | { type: 'input'; item: string }
  | { type: 'output'; item: string };

export interface MachineFlow {
  entity_number: number;
  crafts_per_second: number;
  utilization: number;
  limit: Limit;
}

export interface InserterFlow {
  entity_number: number;
  items_per_second: number;
  capacity: number;
  utilization: number;
}

export interface LaneFlow {
  tile: number;
  lane: number;
  items_per_second: number;
  capacity: number;
}

/** PT: Relatório de uma simulação de fluxo. EN: Report of one flow simulation. */
export interface FlowReport {
  machines: MachineFlow[];
  inserters: InserterFlow[];
  lanes: LaneFlow[];
  produced: Record<string, number>;
  drained: Record<string, number>;
  rounds: number;
  converged: boolean;
}

export type Cause =
  | { type: 'ok' }
  | { type: 'no_recipe' }
  | { type: 'unpowered' }
  | { type: 'missing_input'; item: string }
  | { type: 'slow_inserter'; item: string; inserter: number }
  | { type: 'lane_saturated'; item: string }
  | { type: 'output_blocked'; item: string };

export interface Bottleneck {
  entity_number: number;
  utilization: number;
  cause: Cause;
}

export interface Efficiency {
  efficiency: number;
  machines: Bottleneck[];
}

export interface SankeyNode {
  kind: 'item' | 'recipe';
  name: string;
  loop: boolean;
}

export interface SankeyLink {
  source: number;
  target: number;
  per_second: number;
}

export interface Sankey {
  nodes: SankeyNode[];
  links: SankeyLink[];
}

export interface Analysis {
  tree: TreeNode;
  blueprints: BlueprintSummary[];
  warnings: string[];
  selected: number;
  world: World;
  report: CalcReport;
  flow?: FlowReport;
  efficiency?: Efficiency;
  sankey?: Sankey;
}

/** PT: Opções da análise (espelha `bf_calc::AnalysisOptions`). EN: Analysis options (mirrors `bf_calc::AnalysisOptions`). */
export interface AnalysisOptions {
  recipe_overrides?: Record<string, string>;
  input_limits?: Record<string, number>;
  blocked_outputs?: string[];
  exported?: string[];
  fuel?: string | null;
  inserter_research?: 'none' | 'all';
}

/** PT: Erro vindo do WASM ou da API. EN: Error coming from the WASM or the API. */
export interface ErrorBody {
  code: string;
  message?: string;
}
