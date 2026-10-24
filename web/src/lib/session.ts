// PT: Estado da sessão na interface (usuário logado ou não) e um texto de blueprint pendente para
//     abrir na página de análise.
// EN: Session state in the interface (logged in or not) and a pending blueprint text to open on
//     the analysis page.
import { writable } from 'svelte/store';
import { api, ApiError, type Me } from './api/client';

export const me = writable<Me | null>(null);
export const sessionChecked = writable(false);
/** PT: Texto a abrir na página de análise. EN: Text to open on the analysis page. */
export const pendingSource = writable<string | null>(null);

/** PT: Confere a sessão atual. EN: Checks the current session. */
export async function refreshSession(): Promise<void> {
  try {
    me.set(await api.get<Me>('/auth/me'));
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 401) console.warn('session check failed');
    me.set(null);
  } finally {
    sessionChecked.set(true);
  }
}

/** PT: Sai da conta; o estado só é limpo se o servidor confirmar (ou se a sessão já não valia).
 *  EN: Signs out; the state is only cleared if the server confirms (or if the session was already invalid). */
export async function logout(): Promise<boolean> {
  try {
    await api.post('/auth/logout');
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 401) return false;
  }
  me.set(null);
  return true;
}
