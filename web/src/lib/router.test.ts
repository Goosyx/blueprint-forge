// PT: O roteador só aceita rotas conhecidas e UUIDs válidos. EN: The router only accepts known routes and valid UUIDs.
import { describe, expect, it } from 'vitest';
import { parse } from './router';

describe('router', () => {
  it('parses known routes', () => {
    expect(parse('')).toEqual({ page: 'analyze' });
    expect(parse('#/login')).toEqual({ page: 'login' });
    expect(parse('#/blueprints')).toEqual({ page: 'blueprints' });
    const id = '0190f0a1-2b3c-7d4e-8f90-123456789abc';
    expect(parse(`#/blueprints/${id}`)).toEqual({ page: 'blueprint', id });
    expect(parse(`#/projects/${id}`)).toEqual({ page: 'project', id });
  });

  it('rejects odd ids and unknown routes', () => {
    expect(parse('#/blueprints/../../etc')).toEqual({ page: 'blueprints' });
    expect(parse('#/blueprints/<img>')).toEqual({ page: 'blueprints' });
    expect(parse('#/javascript:alert(1)')).toEqual({ page: 'analyze' });
  });
});
