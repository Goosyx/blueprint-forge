<!--
  PT: Configurações: unidade de taxa preferida e troca de senha.
  EN: Settings: preferred rate unit and password change.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Preferences } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { t } from '../lib/i18n';

  let preferences = $state<Preferences | null>(null);
  let current = $state('');
  let next = $state('');
  let message = $state<string | null>(null);
  let error = $state<unknown>(null);

  onMount(async () => {
    try {
      preferences = await api.get<Preferences>('/preferences');
    } catch (failure) {
      error = failure;
    }
  });

  async function savePreferences(): Promise<void> {
    if (!preferences) return;
    error = null;
    try {
      await api.put('/preferences', preferences);
      message = $t('common.saved');
    } catch (failure) {
      error = failure;
    }
  }

  async function changePassword(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    error = null;
    try {
      await api.post('/auth/password', { current, new: next });
      current = '';
      next = '';
      message = $t('settings.password_changed');
    } catch (failure) {
      error = failure;
    }
  }
</script>

{#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
{#if message}<p class="muted">{message}</p>{/if}
{#if preferences}
  <section class="card form">
    <h2>{$t('settings.title')}</h2>
    <label>
      <span>{$t('analyze.unit')}</span>
      <select bind:value={preferences.rate_unit}>
        <option value="second">/s</option>
        <option value="minute">/min</option>
        <option value="hour">/h</option>
      </select>
    </label>
    <button onclick={savePreferences}>{$t('common.save')}</button>
  </section>
{/if}
<form class="card form" onsubmit={changePassword}>
  <h3>{$t('settings.change_password')}</h3>
  <label>
    <span>{$t('settings.current_password')}</span>
    <input
      type="password"
      bind:value={current}
      autocomplete="current-password"
      required
      maxlength="128"
    />
  </label>
  <label>
    <span>{$t('settings.new_password')}</span>
    <input
      type="password"
      bind:value={next}
      autocomplete="new-password"
      required
      minlength="10"
      maxlength="128"
    />
  </label>
  <button type="submit">{$t('settings.change_password')}</button>
</form>
