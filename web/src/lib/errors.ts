// PT: Texto traduzido de um código de erro (da API ou do motor), com uma mensagem genérica de reserva.
// EN: Translated text for an error code (from the API or the engine), with a generic fallback message.
import { ApiError } from './api/client';
import { errorKey } from './i18n';

type Translate = (key: string, params?: Record<string, string | number>) => string;

export function errorText(t: Translate, error: unknown): string {
  const code =
    error instanceof ApiError ? error.code : typeof error === 'string' ? error : 'internal';
  const key = errorKey(code);
  const text = t(key);
  return text === key ? t('error.generic', { code }) : text;
}
