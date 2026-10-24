// PT: Internacionalização simples: dicionários PT e EN, idioma guardado no navegador e uma
//     função `t` reativa. Os parâmetros entram como texto puro, nunca como HTML.
// EN: Simple internationalization: PT and EN dictionaries, language kept in the browser and a
//     reactive `t` function. Parameters go in as plain text, never as HTML.
import { derived, writable } from 'svelte/store';
import en from './en.json';
import ptBR from './pt-BR.json';

export type Language = 'pt-BR' | 'en';
export type Dictionary = Record<string, string>;

export const dictionaries: Record<Language, Dictionary> = { 'pt-BR': ptBR, en };
export const languages: Language[] = ['pt-BR', 'en'];

const STORAGE_KEY = 'bf-language';

// PT: Lê o idioma salvo; o armazenamento pode estar bloqueado, então tudo fica em try/catch.
// EN: Reads the saved language; storage may be blocked, so everything is in try/catch.
function initialLanguage(): Language {
  try {
    const saved = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (saved === 'pt-BR' || saved === 'en') return saved;
  } catch {
    // PT: Sem acesso ao armazenamento. EN: No storage access.
  }
  const browser = globalThis.navigator?.language ?? 'pt-BR';
  return browser.toLowerCase().startsWith('pt') ? 'pt-BR' : 'en';
}

export const language = writable<Language>(initialLanguage());

language.subscribe((value) => {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, value);
  } catch {
    // PT: Ignora: a preferência só não fica salva. EN: Ignore: the preference just is not saved.
  }
  if (globalThis.document) globalThis.document.documentElement.lang = value;
});

/**
 * PT: Traduz `key` no idioma dado, trocando `{nome}` pelos parâmetros. Chave ausente devolve a
 *     própria chave, para aparecer na tela e ser corrigida.
 * EN: Translates `key` into the given language, replacing `{name}` with the parameters. A
 *     missing key returns the key itself, so it shows up on screen and gets fixed.
 */
export function translate(
  lang: Language,
  key: string,
  params: Record<string, string | number> = {},
): string {
  const template = dictionaries[lang][key] ?? dictionaries.en[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

export const t = derived(
  language,
  (lang) => (key: string, params?: Record<string, string | number>) => translate(lang, key, params),
);

/** PT: Mensagem de erro traduzida a partir do código. EN: Translated error message from its code. */
export function errorKey(code: string): string {
  return `error.${code}`;
}
