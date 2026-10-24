// PT: Toda chave existe nos dois idiomas, e os parâmetros são trocados como texto.
// EN: Every key exists in both languages, and parameters are replaced as text.
import { describe, expect, it } from 'vitest';
import { dictionaries, translate } from './index';

describe('i18n', () => {
  it('has the same keys in pt-BR and en', () => {
    const pt = Object.keys(dictionaries['pt-BR']).sort();
    const en = Object.keys(dictionaries.en).sort();
    expect(pt).toEqual(en);
  });

  it('has no empty translations', () => {
    for (const dictionary of Object.values(dictionaries)) {
      for (const [key, value] of Object.entries(dictionary)) {
        expect(value.trim(), key).not.toBe('');
      }
    }
  });

  it('replaces parameters and keeps unknown ones', () => {
    expect(translate('en', 'analyze.entities', { count: 3 })).toBe('3 entities');
    expect(translate('pt-BR', 'analyze.entities', { count: 3 })).toBe('3 entidades');
    expect(translate('en', 'analyze.entities')).toBe('{count} entities');
    expect(translate('en', 'missing.key')).toBe('missing.key');
  });

  it('covers every Rust decode error code', () => {
    const codes = [
      'blueprint.empty',
      'blueprint.too_large',
      'blueprint.unsupported_version',
      'blueprint.invalid_character',
      'blueprint.invalid_base64',
      'blueprint.invalid_compression',
      'blueprint.decompressed_too_large',
      'blueprint.compression_ratio',
      'blueprint.invalid_utf8',
      'blueprint.invalid_json',
      'blueprint.not_a_blueprint',
      'blueprint.limit_exceeded',
      'blueprint.book_too_deep',
      'blueprint.invalid_position',
      'blueprint.area_too_large',
      'blueprint.duplicate_entity_number',
      'blueprint.invalid_entity_number',
      'blueprint.invalid_direction',
      'blueprint.dangling_wire',
      'blueprint.text_too_long',
      'world.too_many_tiles',
    ];
    for (const code of codes) {
      expect(dictionaries.en[`error.${code}`], code).toBeTruthy();
    }
  });
});
