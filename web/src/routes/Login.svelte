<!--
  PT: Entrar com e-mail e senha. A sessão fica num cookie HttpOnly; a página nunca vê o token.
  EN: Sign in with email and password. The session lives in an HttpOnly cookie; the page never sees the token.
-->
<script lang="ts">
  import { api } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { t } from '../lib/i18n';
  import { go } from '../lib/router';
  import { refreshSession } from '../lib/session';

  let email = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<unknown>(null);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    busy = true;
    error = null;
    try {
      await api.post('/auth/login', { email, password });
      password = '';
      await refreshSession();
      go('/blueprints');
    } catch (failure) {
      error = failure;
    } finally {
      busy = false;
    }
  }
</script>

<form class="card form" onsubmit={submit}>
  <h2>{$t('auth.login_title')}</h2>
  <label>
    <span>{$t('auth.email')}</span>
    <input type="email" bind:value={email} autocomplete="username" required maxlength="254" />
  </label>
  <label>
    <span>{$t('auth.password')}</span>
    <input
      type="password"
      bind:value={password}
      autocomplete="current-password"
      required
      maxlength="128"
    />
  </label>
  {#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
  <button type="submit" disabled={busy}>{$t('auth.login')}</button>
  <p class="muted"><a href="#/register">{$t('auth.go_register')}</a></p>
</form>
