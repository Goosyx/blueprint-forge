// PT: Roteador por hash (`#/caminho`), sem dependências. Só aceita rotas conhecidas e ids no
//     formato UUID; qualquer outra coisa cai na página de análise.
// EN: Hash router (`#/path`), without dependencies. It only accepts known routes and ids in UUID
//     format; anything else falls back to the analysis page.
import { readable } from 'svelte/store';

export type Route =
  | { page: 'analyze' }
  | { page: 'login' }
  | { page: 'register' }
  | { page: 'blueprints' }
  | { page: 'blueprint'; id: string }
  | { page: 'projects' }
  | { page: 'project'; id: string }
  | { page: 'settings' };

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** PT: Converte o hash numa rota. EN: Turns the hash into a route. */
export function parse(hash: string): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean);
  const [first, second] = parts;
  if (first === 'login') return { page: 'login' };
  if (first === 'register') return { page: 'register' };
  if (first === 'settings') return { page: 'settings' };
  if (first === 'blueprints') {
    return second && UUID.test(second) ? { page: 'blueprint', id: second } : { page: 'blueprints' };
  }
  if (first === 'projects') {
    return second && UUID.test(second) ? { page: 'project', id: second } : { page: 'projects' };
  }
  return { page: 'analyze' };
}

export const route = readable<Route>(parse(globalThis.location?.hash ?? ''), (set) => {
  const update = () => set(parse(globalThis.location.hash));
  globalThis.addEventListener?.('hashchange', update);
  return () => globalThis.removeEventListener?.('hashchange', update);
});

/** PT: Navega para um caminho. EN: Navigates to a path. */
export function go(path: string): void {
  globalThis.location.hash = `#${path}`;
}
