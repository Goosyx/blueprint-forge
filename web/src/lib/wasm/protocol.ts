// PT: Mensagens trocadas entre a interface e o Web Worker do motor.
// EN: Messages exchanged between the interface and the engine Web Worker.
import type { ErrorBody } from '../types';

export type WorkerRequest =
  | { type: 'init'; gameData: string }
  | { type: 'analyze'; id: number; text: string; selected: number; options: string };

export type WorkerResponse =
  | { type: 'ready'; gameVersion: string }
  | { type: 'result'; id: number; json: string }
  | { type: 'error'; id?: number; error: ErrorBody; fatal: boolean };
