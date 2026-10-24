// PT: Web Worker que roda o motor WASM fora da thread da interface. Recebe mensagens `init` e
//     `analyze` e responde com JSON em texto. Um trap do WASM (pânico, falta de memória) é fatal:
//     o worker avisa e se encerra, porque a instância fica num estado indefinido.
// EN: Web Worker that runs the WASM engine off the UI thread. It receives `init` and `analyze`
//     messages and replies with JSON text. A WASM trap (panic, out of memory) is fatal: the
//     worker reports it and closes itself, because the instance is left in an undefined state.
import init, { Engine } from './pkg/bf_wasm.js';
import wasmUrl from './pkg/bf_wasm_bg.wasm?url';
import type { WorkerRequest, WorkerResponse } from './protocol';

let engine: Engine | null = null;

function reply(message: WorkerResponse): void {
  self.postMessage(message);
}

// PT: Erro esperado do motor: mensagem com JSON `{code, message}`. Qualquer outra coisa é trap.
// EN: Expected engine error: a message holding `{code, message}` JSON. Anything else is a trap.
function expectedError(error: unknown): { code: string; message?: string } | null {
  if (!(error instanceof Error) || error instanceof WebAssembly.RuntimeError) return null;
  try {
    const parsed: unknown = JSON.parse(error.message);
    if (parsed && typeof parsed === 'object' && 'code' in parsed) {
      const body = parsed as { code: unknown; message?: unknown };
      if (typeof body.code === 'string') {
        return {
          code: body.code,
          message: typeof body.message === 'string' ? body.message : undefined,
        };
      }
    }
  } catch {
    // PT: Não era JSON. EN: It was not JSON.
  }
  return null;
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  const id = request.type === 'analyze' ? request.id : undefined;
  try {
    if (request.type === 'init') {
      await init({ module_or_path: wasmUrl });
      engine = new Engine(request.gameData);
      reply({ type: 'ready', gameVersion: engine.gameVersion });
    } else if (request.type === 'analyze') {
      if (!engine) throw new Error(JSON.stringify({ code: 'engine.not_ready' }));
      reply({
        type: 'result',
        id: request.id,
        json: engine.analyze(request.text, request.selected, request.options),
      });
    }
  } catch (error) {
    const body = expectedError(error);
    if (body) {
      reply({ type: 'error', id, error: body, fatal: false });
    } else {
      reply({ type: 'error', id, error: { code: 'engine.crashed' }, fatal: true });
      self.close();
    }
  }
};
