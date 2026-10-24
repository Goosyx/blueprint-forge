// PT: Formatação de taxas, potência, energia e porcentagem para exibição, no idioma escolhido.
// EN: Formatting of rates, power, energy and percentage for display, in the chosen language.
import type { Language } from './i18n';

export type RateUnit = 'second' | 'minute' | 'hour';

const UNIT_FACTOR: Record<RateUnit, number> = { second: 1, minute: 60, hour: 3600 };
const UNIT_SUFFIX: Record<RateUnit, string> = { second: '/s', minute: '/min', hour: '/h' };

function number(lang: Language, value: number, digits: number): string {
  return new Intl.NumberFormat(lang, { maximumFractionDigits: digits }).format(value);
}

/** PT: Taxa por segundo convertida para a unidade escolhida. EN: Per-second rate converted to the chosen unit. */
export function formatRate(lang: Language, perSecond: number, unit: RateUnit): string {
  const value = perSecond * UNIT_FACTOR[unit];
  const digits = Math.abs(value) >= 100 ? 0 : Math.abs(value) >= 10 ? 1 : 2;
  return `${number(lang, value, digits)}${UNIT_SUFFIX[unit]}`;
}

function scaled(lang: Language, value: number, unit: string): string {
  const prefixes = ['', 'k', 'M', 'G', 'T'];
  let scaledValue = value;
  let index = 0;
  while (Math.abs(scaledValue) >= 1000 && index < prefixes.length - 1) {
    scaledValue /= 1000;
    index += 1;
  }
  return `${number(lang, scaledValue, scaledValue >= 100 ? 0 : 2)} ${prefixes[index]}${unit}`;
}

/** PT: Potência em W com prefixo do SI. EN: Power in W with an SI prefix. */
export function formatPower(lang: Language, watts: number): string {
  return scaled(lang, watts, 'W');
}

/** PT: Energia em J com prefixo do SI. EN: Energy in J with an SI prefix. */
export function formatEnergy(lang: Language, joules: number): string {
  return scaled(lang, joules, 'J');
}

/** PT: Fração (0,5) como porcentagem (50%). EN: Fraction (0.5) as a percentage (50%). */
export function formatPercent(lang: Language, fraction: number): string {
  return new Intl.NumberFormat(lang, { style: 'percent', maximumFractionDigits: 1 }).format(
    fraction,
  );
}
