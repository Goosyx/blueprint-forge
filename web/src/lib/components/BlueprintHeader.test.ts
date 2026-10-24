// PT: Um nome malicioso aparece como texto literal, sem virar HTML.
// EN: A malicious name shows up as literal text, without becoming HTML.
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import BlueprintHeader from './BlueprintHeader.svelte';

describe('BlueprintHeader', () => {
  it('escapes labels and descriptions', () => {
    const { body } = render(BlueprintHeader, {
      props: {
        label: '<img src=x onerror=alert(1)>',
        description: '<script>alert(2)</script>[item=iron-plate]',
        untitled: 'Untitled',
      },
    });
    expect(body).not.toContain('<img');
    expect(body).not.toContain('<script>');
    expect(body).toContain('&lt;img src=x onerror=alert(1)>');
    expect(body).toContain('&lt;script>alert(2)&lt;/script>[item=iron-plate]');
  });

  it('falls back to the untitled text', () => {
    const { body } = render(BlueprintHeader, {
      props: { label: null, description: null, untitled: 'Sem nome' },
    });
    expect(body).toContain('Sem nome');
  });
});
