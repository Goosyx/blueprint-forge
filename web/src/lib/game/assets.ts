// PT: Carrega o pacote do jogo servido pela API: dados, nomes traduzidos e atlas de ícones.
//     Os ícones são opcionais: sem eles a interface desenha caixas coloridas.
// EN: Loads the game package served by the API: data, translated names and the icon atlas.
//     Icons are optional: without them the interface draws colored boxes.
import type { Language } from '../i18n';

const API = '/api/v1';

export interface GameVersion {
  version: string;
  has_icons: boolean;
  unvalidated: boolean;
}

export interface Slot {
  page: number;
  col: number;
  row: number;
}

export interface Atlas {
  cell: number;
  pages: string[];
  icons: Record<string, Slot>;
}

export interface GameLocale {
  language: string;
  items: Record<string, string>;
  fluids: Record<string, string>;
  recipes: Record<string, string>;
  entities: Record<string, string>;
  virtual_signals: Record<string, string>;
}

async function fetchOk(url: string): Promise<Response> {
  const response = await fetch(url, { credentials: 'same-origin' });
  if (!response.ok) throw new Error(`${response.status} ${url}`);
  return response;
}

/** PT: Versões disponíveis no servidor. EN: Versions available on the server. */
export async function fetchVersions(): Promise<GameVersion[]> {
  return (await fetchOk(`${API}/game/versions`)).json() as Promise<GameVersion[]>;
}

/** PT: Conteúdo do `gamedata.json`, como texto, para o WASM. EN: `gamedata.json` contents, as text, for the WASM. */
export async function fetchGameData(version: string): Promise<string> {
  return (await fetchOk(`${API}/game/${encodeURIComponent(version)}/data`)).text();
}

/** PT: Nomes traduzidos de um idioma. EN: Translated names for one language. */
export async function fetchLocale(version: string, language: Language): Promise<GameLocale> {
  const url = `${API}/game/${encodeURIComponent(version)}/locale/${encodeURIComponent(language)}`;
  return (await fetchOk(url)).json() as Promise<GameLocale>;
}

/** PT: Índice do atlas, ou `null` se o pacote não tem ícones. EN: Atlas index, or `null` if the package has no icons. */
export async function fetchAtlas(version: string): Promise<Atlas | null> {
  try {
    const url = `${API}/assets/icons/${encodeURIComponent(version)}/atlas.json`;
    return (await fetchOk(url)).json() as Promise<Atlas>;
  } catch {
    return null;
  }
}

/** PT: URL de uma página do atlas. EN: URL of one atlas page. */
export function atlasPageUrl(version: string, page: string): string {
  return `${API}/assets/icons/${encodeURIComponent(version)}/${encodeURIComponent(page)}`;
}

/** PT: Nome de exibição de uma entidade. EN: Display name of an entity. */
export function entityName(locale: GameLocale | null, name: string): string {
  return locale?.entities[name] ?? locale?.items[name] ?? name;
}

/** PT: Nome de exibição de uma receita. EN: Display name of a recipe. */
export function recipeName(locale: GameLocale | null, name: string): string {
  return locale?.recipes[name] ?? locale?.items[name] ?? locale?.fluids[name] ?? name;
}
