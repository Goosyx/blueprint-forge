// PT: Formatação de números nos dois idiomas. EN: Number formatting in both languages.
import { describe, expect, it } from 'vitest';
import { formatEnergy, formatPercent, formatPower, formatRate } from './format';

describe('format', () => {
  it('converts rate units', () => {
    expect(formatRate('en', 0.5, 'second')).toBe('0.5/s');
    expect(formatRate('en', 0.5, 'minute')).toBe('30/min');
    expect(formatRate('pt-BR', 1.25, 'second')).toBe('1,25/s');
    expect(formatRate('en', 1, 'hour')).toBe('3,600/h');
  });

  it('scales power and energy', () => {
    expect(formatPower('en', 150_000)).toBe('150 kW');
    expect(formatPower('en', 5_820_000)).toBe('5.82 MW');
    expect(formatPower('pt-BR', 525_000)).toBe('525 kW');
    expect(formatEnergy('en', 5_000_000)).toBe('5 MJ');
  });

  it('formats percentages', () => {
    expect(formatPercent('en', 2 / 3)).toBe('66.7%');
  });
});
