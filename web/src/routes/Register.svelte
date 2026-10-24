<!--
  PT: Criar conta. O servidor decide se o cadastro está fechado, por convite ou aberto.
  EN: Create an account. The server decides whether registration is closed, by invite or open.
-->
<script lang="ts">
  import { api } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { language, t } from '../lib/i18n';
  import { go } from '../lib/router';

  let email = $state('');
  let password = $state('');
  let invite = $state('');
  let busy = $state(false);
  let error = $state<unknown>(null);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    busy = true;
    error = null;
    try {
      await api.post('/auth/register', {
        email,
        password,
        locale: $language,
        invite: invite.trim() || undefined,
      });
      go('/login');
    } catch (failure) {
      error = failure;
    } finally {
      busy = false;
    }
  }
</script>

<form class="card form" onsubmit={submit}>
  <h2>{$t('auth.register_title')}</h2>
  <label>
    <span>{$t('auth.email')}</span>
    <input type="email" bind:value={email} autocomplete="username" required maxlength="254" />
  </label>
  <label>
    <span>{$t('auth.password')}</span>
    <input
      type="password"
      bind:value={password}
      autocomplete="new-password"
      required
      minlength="10"
      maxlength="128"
    />
  </label>
  <label>
    <span>{$t('auth.invite')}</span>
    <input type="text" bind:value={invite} autocomplete="off" maxlength="64" />
  </label>
  {#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
  <button type="submit" disabled={busy}>{$t('auth.register')}</button>
</form>
