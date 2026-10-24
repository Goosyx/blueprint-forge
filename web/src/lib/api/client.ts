// PT: Cliente da API: mesma origem, cookie de sessão HttpOnly enviado pelo navegador e erros no
//     formato `{error: {code}}` convertidos em `ApiError` com código traduzível.
// EN: API client: same origin, HttpOnly session cookie sent by the browser and errors in the
//     `{error: {code}}` format turned into an `ApiError` with a translatable code.

const API = '/api/v1';

/** PT: Erro da API com código traduzível e status HTTP. EN: API error with a translatable code and HTTP status. */
export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, status: number) {
    super(code);
    this.code = code;
    this.status = status;
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${API}${path}`, {
      method,
      credentials: 'same-origin',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError('network', 0);
  }
  if (!response.ok) {
    let code = `http.${response.status}`;
    try {
      const parsed = (await response.json()) as { error?: { code?: unknown } };
      if (typeof parsed.error?.code === 'string') code = parsed.error.code;
    } catch {
      // PT: Corpo sem JSON. EN: Body without JSON.
    }
    throw new ApiError(code, response.status);
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body ?? {}),
  put: <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  patch: <T>(path: string, body: unknown) => request<T>('PATCH', path, body),
  delete: <T>(path: string) => request<T>('DELETE', path),
};

export interface Me {
  id: string;
  email: string;
  locale: string;
}

export interface BlueprintItem {
  id: string;
  title: string;
  description: string | null;
  is_book: boolean;
  blueprint_count: number;
  entity_count: number;
  created_at: string;
  updated_at: string;
}

export interface SavedSummary {
  is_book: boolean;
  blueprint_count: number;
  entity_count: number;
  label: string | null;
  selected: number;
  selected_entities: number;
  outputs: [string, number][];
  power_average_w: number;
  power_peak_w: number;
  pollution_per_minute: number;
  warnings: number;
}

export interface SavedAnalysis {
  id: string;
  selected: number;
  options: unknown;
  summary: SavedSummary;
  data_version: string;
  created_at: string;
}

export interface BlueprintDetail extends BlueprintItem {
  source: string;
  analyses: SavedAnalysis[];
}

export interface Project {
  id: string;
  name: string;
  description: string | null;
  blueprints: number;
  created_at: string;
  updated_at: string;
}

export interface ProjectDetail extends Project {
  items: { id: string; title: string; position: number; note: string | null }[];
}

export interface Preferences {
  rate_unit: 'second' | 'minute' | 'hour';
  analysis_defaults: Record<string, unknown>;
  generator_defaults: Record<string, unknown>;
}
