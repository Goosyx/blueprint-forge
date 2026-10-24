// PT: Estado compartilhado do jogo na interface: atlas de ícones, nomes traduzidos e um índice
//     leve dos dados (receitas por categoria e categorias de cada máquina).
// EN: Shared game state in the interface: icon atlas, translated names and a light index of the
//     data (recipes by category and the categories of each machine).
import { writable } from 'svelte/store';
import type { Atlas, GameLocale } from './assets';

export interface AtlasState {
  atlas: Atlas;
  pageUrl: (page: string) => string;
}

export interface GameIndex {
  /** PT: Categoria → receitas. EN: Category → recipes. */
  recipesByCategory: Record<string, string[]>;
  /** PT: Máquina → categorias que ela faz. EN: Machine → categories it makes. */
  machineCategories: Record<string, string[]>;
}

export const atlasState = writable<AtlasState | null>(null);
export const gameLocale = writable<GameLocale | null>(null);
export const gameIndex = writable<GameIndex | null>(null);

interface RawGameData {
  recipes?: Record<string, { categories?: string[]; hidden?: boolean }>;
  entities?: Record<string, { kind?: { type?: string; crafting_categories?: string[] } }>;
}

/** PT: Monta o índice a partir do texto do `gamedata.json`. EN: Builds the index from the `gamedata.json` text. */
export function buildGameIndex(text: string): GameIndex {
  const data = JSON.parse(text) as RawGameData;
  const recipesByCategory: Record<string, string[]> = {};
  for (const [name, recipe] of Object.entries(data.recipes ?? {})) {
    if (recipe.hidden) continue;
    for (const category of recipe.categories ?? ['crafting']) {
      (recipesByCategory[category] ??= []).push(name);
    }
  }
  const machineCategories: Record<string, string[]> = {};
  for (const [name, entity] of Object.entries(data.entities ?? {})) {
    if (entity.kind?.type === 'crafting-machine') {
      machineCategories[name] = entity.kind.crafting_categories ?? [];
    }
  }
  return { recipesByCategory, machineCategories };
}
