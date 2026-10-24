// PT: Cores por categoria de entidade e regras de seta de direção do desenho.
// EN: Colors by entity category and direction-arrow rules for the drawing.

const COLORS: Record<string, number> = {
  'assembling-machine': 0x3a6ea5,
  furnace: 0xa5603a,
  'rocket-silo': 0x3a6ea5,
  inserter: 0xc9a227,
  'transport-belt': 0x9c8a3c,
  'underground-belt': 0x9c8a3c,
  splitter: 0x9c8a3c,
  loader: 0x9c8a3c,
  'loader-1x1': 0x9c8a3c,
  'electric-pole': 0x7a5230,
  pipe: 0x4a7a8c,
  'pipe-to-ground': 0x4a7a8c,
  pump: 0x4a7a8c,
  'storage-tank': 0x4a7a8c,
  'offshore-pump': 0x4a7a8c,
  container: 0x6b5b4b,
  'logistic-container': 0x8a6b4b,
  beacon: 0x7d4ea8,
  boiler: 0x3f8f4f,
  generator: 0x3f8f4f,
  'solar-panel': 0x3f8f4f,
  accumulator: 0x3f8f4f,
  reactor: 0x3f8f4f,
  'mining-drill': 0x8c7a4a,
  lab: 0x4a8c8c,
};

/** PT: Cor neutra para os demais tipos. EN: Neutral color for other types. */
const DEFAULT_COLOR = 0x5a5d66;
/** PT: Vermelho para entidades desconhecidas. EN: Red for unknown entities. */
const UNKNOWN_COLOR = 0xb33a3a;

const DIRECTIONAL = new Set([
  'inserter',
  'transport-belt',
  'underground-belt',
  'splitter',
  'loader',
  'loader-1x1',
  'pump',
  'offshore-pump',
  'boiler',
]);

/** PT: Cor do retângulo da entidade. EN: Entity rectangle color. */
export function categoryColor(prototype: string | null): number {
  if (prototype === null) return UNKNOWN_COLOR;
  return COLORS[prototype] ?? DEFAULT_COLOR;
}

/** PT: Se a entidade ganha seta de direção. EN: Whether the entity gets a direction arrow. */
export function isDirectional(prototype: string | null): boolean {
  return prototype !== null && DIRECTIONAL.has(prototype);
}

/**
 * PT: Direção da seta em 16 passos. No insersor a direção da blueprint aponta para o lado de onde
 *     ele pega, então a seta (pega → entrega) aponta para o lado oposto.
 * EN: Arrow direction in 16 steps. For inserters the blueprint direction points to the side they
 *     pick up from, so the arrow (pick up → drop) points the opposite way.
 */
export function arrowDirection(prototype: string | null, direction: number): number {
  return prototype === 'inserter' ? (direction + 8) % 16 : direction;
}
