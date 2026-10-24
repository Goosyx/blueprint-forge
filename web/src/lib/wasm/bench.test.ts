// PT: Tempo do motor WASM nas blueprints sintéticas de `bf-validate bench-sim` (target/bench).
//     Só roda com BF_BENCH=1; o resultado vai para validation/measurements/sim-benchmarks.md.
// EN: WASM engine time on the synthetic blueprints from `bf-validate bench-sim` (target/bench).
//     Only runs with BF_BENCH=1; the result goes into validation/measurements/sim-benchmarks.md.
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { Engine, initSync } from './pkg/bf_wasm.js';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const version = /FACTORIO_VERSION="([^"]+)"/.exec(
  readFileSync(here('../../../../scripts/factorio-version.sh'), 'utf8'),
)?.[1];
const gameData = here(`../../../../game-assets/${version}/gamedata.json`);

function time(run: () => void): number {
  const times: number[] = [];
  for (let i = 0; i < 5; i++) {
    const started = performance.now();
    run();
    times.push(performance.now() - started);
  }
  times.sort((a, b) => a - b);
  return times[2] ?? 0;
}

describe.runIf(process.env.BF_BENCH === '1')('wasm bench', () => {
  it('times the analysis and the tick simulation', () => {
    initSync({ module: readFileSync(here('./pkg/bf_wasm_bg.wasm')) });
    const engine = new Engine(readFileSync(gameData, 'utf8'));
    for (const rows of [1, 10, 95]) {
      const file = here(`../../../../target/bench/rows-${rows}.txt`);
      if (!existsSync(file)) continue;
      const text = readFileSync(file, 'utf8');
      const analysis = time(() => engine.analyze(text, 0, '{}'));
      const tick = time(() => engine.simulate(text, 0, '{}', 600, 3600));
      console.log(`rows ${rows}: analyze ${analysis.toFixed(1)} ms, tick ${tick.toFixed(1)} ms`);
      expect(analysis).toBeGreaterThan(0);
    }
  }, 120_000);
});
