// PT: Cliente do motor: cria o Web Worker, envia pedidos e aplica um tempo limite. Se a análise
//     passar do limite ou o worker sofrer um trap, ele é recriado, com no máximo 3 tentativas
//     seguidas e espera crescente; depois disso o motor fica em falha até recarregar a página.
// EN: Engine client: creates the Web Worker, sends requests and applies a timeout. If the analysis
//     exceeds the limit or the worker traps, it is recreated, with at most 3 attempts in a row and
//     a growing delay; after that the engine stays failed until the page is reloaded.
import type { Analysis, AnalysisOptions, ErrorBody, FlowReport } from '../types';
import type { WorkerRequest, WorkerResponse } from './protocol';

/** PT: Tempo máximo de uma análise. EN: Maximum time for one analysis. */
const ANALYZE_TIMEOUT_MS = 20_000;
/** PT: Tempo máximo da simulação por tick. EN: Maximum time for the tick simulation. */
const SIMULATE_TIMEOUT_MS = 180_000;
/** PT: Tentativas seguidas de recriação. EN: Recreation attempts in a row. */
const MAX_RESTARTS = 3;

/** PT: Erro com código traduzível. EN: Error with a translatable code. */
export class EngineError extends Error {
  readonly code: string;

  constructor(body: ErrorBody) {
    super(body.message ?? body.code);
    this.code = body.code;
  }
}

interface Pending {
  resolve: (json: string) => void;
  reject: (error: EngineError) => void;
  timer: ReturnType<typeof setTimeout>;
}

export class EngineClient {
  private worker: Worker | null = null;
  private gameData: string | null = null;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private ready: Promise<string> | null = null;
  private restarts = 0;
  private failed = false;

  /** PT: Carrega os dados do jogo no worker. EN: Loads the game data into the worker. */
  start(gameData: string): Promise<string> {
    this.gameData = gameData;
    this.restarts = 0;
    this.failed = false;
    return this.spawn();
  }

  /** PT: Analisa uma blueprint string. EN: Analyzes a blueprint string. */
  async analyze(text: string, selected: number, options: AnalysisOptions): Promise<Analysis> {
    if (this.failed || !this.ready) throw new EngineError({ code: 'engine.not_ready' });
    // PT: Espera o `ready` atual (o worker pode ter sido recriado). EN: Wait for the current `ready` (the worker may have been recreated).
    await this.ready;
    const json = await this.send(
      (id) => ({ type: 'analyze', id, text, selected, options: JSON.stringify(options) }),
      ANALYZE_TIMEOUT_MS,
    );
    return JSON.parse(json) as Analysis;
  }

  /**
   * PT: Simulação tick a tick (lenta). `cancel()` interrompe recriando o worker.
   * EN: Tick-by-tick simulation (slow). `cancel()` interrupts it by recreating the worker.
   */
  async simulate(
    text: string,
    selected: number,
    options: AnalysisOptions,
    warmup: number,
    measure: number,
  ): Promise<FlowReport> {
    if (this.failed || !this.ready) throw new EngineError({ code: 'engine.not_ready' });
    await this.ready;
    const json = await this.send(
      (id) => ({
        type: 'simulate',
        id,
        text,
        selected,
        options: JSON.stringify(options),
        warmup,
        measure,
      }),
      SIMULATE_TIMEOUT_MS,
    );
    return JSON.parse(json) as FlowReport;
  }

  /** PT: Cancela o que estiver rodando. EN: Cancels whatever is running. */
  cancel(): void {
    if (this.gameData === null || this.failed) return;
    void this.spawn().catch(() => undefined);
  }

  /** PT: Encerra o worker. EN: Shuts the worker down. */
  stop(): void {
    this.worker?.terminate();
    this.worker = null;
    this.ready = null;
    for (const [, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(new EngineError({ code: 'engine.stopped' }));
    }
    this.pending.clear();
  }

  private spawn(): Promise<string> {
    this.stop();
    const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker = worker;
    const ready = new Promise<string>((resolve, reject) => {
      worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
        const message = event.data;
        if (message.type === 'ready') {
          this.restarts = 0;
          resolve(message.gameVersion);
        } else if (message.type === 'error' && message.id === undefined) {
          reject(new EngineError(message.error));
          if (message.fatal) this.restart();
        } else {
          this.settle(message);
        }
      };
      worker.onerror = () => {
        reject(new EngineError({ code: 'engine.crashed' }));
        this.restart();
      };
    });
    this.ready = ready;
    const init: WorkerRequest = { type: 'init', gameData: this.gameData ?? '' };
    worker.postMessage(init);
    return ready;
  }

  private send(build: (id: number) => WorkerRequest, timeoutMs: number): Promise<string> {
    // PT: Sem worker (esperando a recriação), recusa na hora em vez de esperar o tempo limite.
    // EN: Without a worker (waiting for the recreation), refuses right away instead of waiting for the timeout.
    if (!this.worker) return Promise.reject(new EngineError({ code: 'engine.not_ready' }));
    const id = this.nextId++;
    return new Promise<string>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new EngineError({ code: 'engine.timeout' }));
        // PT: O WASM não pode ser interrompido por dentro; recria o worker.
        // EN: The WASM cannot be interrupted from inside; recreate the worker.
        this.restart();
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.worker?.postMessage(build(id));
    });
  }

  private settle(message: WorkerResponse): void {
    if (message.type === 'ready' || message.id === undefined) return;
    const pending = this.pending.get(message.id);
    if (!pending) return;
    clearTimeout(pending.timer);
    this.pending.delete(message.id);
    if (message.type === 'result') {
      pending.resolve(message.json);
    } else {
      pending.reject(new EngineError(message.error));
      if (message.fatal) this.restart();
    }
  }

  // PT: Recria o worker com espera de 1 s, 2 s e 4 s; depois desiste.
  // EN: Recreates the worker with 1 s, 2 s and 4 s delays; then gives up.
  private restart(): void {
    if (this.gameData === null || this.failed) return;
    this.stop();
    if (this.restarts >= MAX_RESTARTS) {
      this.failed = true;
      return;
    }
    const delay = 1000 * 2 ** this.restarts;
    this.restarts += 1;
    setTimeout(() => {
      if (!this.failed && this.worker === null) void this.spawn().catch(() => undefined);
    }, delay);
  }
}
