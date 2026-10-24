// PT: Carrega o WASM real no Node e confere que o JSON segue os tipos de `types.ts`.
//     Usa o pacote do jogo se existir; senão, dados mínimos sintéticos.
// EN: Loads the real WASM in Node and checks that the JSON follows the `types.ts` types.
//     Uses the game package if present; otherwise, minimal synthetic data.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { Engine, initSync } from './pkg/bf_wasm.js';
import type { Analysis } from '../types';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
// PT: A versão vem do mesmo arquivo que os scripts usam. EN: The version comes from the same file the scripts use.
const version = /FACTORIO_VERSION="([^"]+)"/.exec(
  readFileSync(here('../../../../scripts/factorio-version.sh'), 'utf8'),
)?.[1];
const gameData = process.env.BF_GAME_ASSETS
  ? `${process.env.BF_GAME_ASSETS}/gamedata.json`
  : here(`../../../../game-assets/${version}/gamedata.json`);
const fixture = here('../../../../crates/bf-blueprint/tests/fixtures/old_project_green_book.txt');

const SYNTHETIC = JSON.stringify({
  format_version: 2,
  game_version: 'test',
  items: {},
  fluids: {},
  recipes: {},
  entities: {},
  tiles: {},
});

let engine: Engine;

beforeAll(() => {
  initSync({ module: readFileSync(here('./pkg/bf_wasm_bg.wasm')) });
  engine = new Engine(existsSync(gameData) ? readFileSync(gameData, 'utf8') : SYNTHETIC);
});

function codeOf(run: () => unknown): string {
  try {
    run();
  } catch (error) {
    return (JSON.parse((error as Error).message) as { code: string }).code;
  }
  throw new Error('should have failed');
}

describe('wasm engine', () => {
  it('analyzes a book and returns the documented shape', () => {
    const text = readFileSync(fixture, 'utf8');
    const analysis = JSON.parse(engine.analyze(text, 0, '{}')) as Analysis;
    expect(analysis.tree.type).toBe('book');
    expect(analysis.blueprints.length).toBeGreaterThan(1);
    expect(analysis.selected).toBe(0);
    expect(analysis.blueprints[0]!.entity_count).toBe(analysis.world.entities.length);
    const entity = analysis.world.entities[0]!;
    expect(Object.keys(entity.footprint).sort()).toEqual(['height', 'width', 'x', 'y']);
    expect(Array.isArray(analysis.report.machines)).toBe(true);
    expect(typeof analysis.report.power.average_w).toBe('number');
    expect(typeof analysis.report.steady_state.solved).toBe('boolean');

    const second = JSON.parse(engine.analyze(text, 1, '')) as Analysis;
    expect(second.selected).toBe(1);
  });

  it('applies recipe overrides', () => {
    if (!existsSync(gameData)) return;
    const text = readFileSync(fixture, 'utf8');
    const book = JSON.parse(engine.analyze(text, 0, '{}')) as Analysis;
    const last = book.blueprints.length - 1;
    const furnaces = JSON.parse(engine.analyze(text, last, '{}')) as Analysis;
    const furnace = furnaces.report.machines.find((m) => m.name === 'stone-furnace');
    if (!furnace) return;
    const options = JSON.stringify({
      recipe_overrides: { [String(furnace.entity_number)]: 'iron-plate' },
    });
    const fixed = JSON.parse(engine.analyze(text, last, options)) as Analysis;
    const machine = fixed.report.machines.find((m) => m.entity_number === furnace.entity_number);
    expect(machine?.recipe).toBe('iron-plate');
  });

  it('reports errors as a {code, message} JSON', () => {
    expect(codeOf(() => engine.analyze('not a blueprint', 0, ''))).toBe(
      'blueprint.unsupported_version',
    );
    const text = readFileSync(fixture, 'utf8');
    expect(codeOf(() => engine.analyze(text, 99, ''))).toBe('analysis.no_such_blueprint');
    expect(codeOf(() => engine.analyze(text, 0, '{"unknown": 1}'))).toBe(
      'analysis.options_invalid',
    );
  });
});
