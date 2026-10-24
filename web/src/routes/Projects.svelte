<!--
  PT: Projetos de fábrica: criar e listar.
  EN: Factory projects: create and list.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Project } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { t } from '../lib/i18n';

  let items = $state<Project[]>([]);
  let name = $state('');
  let error = $state<unknown>(null);

  async function load(): Promise<void> {
    try {
      items = await api.get<Project[]>('/projects?limit=100');
    } catch (failure) {
      error = failure;
    }
  }

  async function create(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    error = null;
    try {
      await api.post('/projects', { name });
      name = '';
      await load();
    } catch (failure) {
      error = failure;
    }
  }

  async function remove(item: Project): Promise<void> {
    if (!confirm($t('projects.confirm_delete', { name: item.name }))) return;
    try {
      await api.delete(`/projects/${item.id}`);
      await load();
    } catch (failure) {
      error = failure;
    }
  }

  onMount(() => void load());
</script>

<section class="card">
  <h2>{$t('projects.title')}</h2>
  <form class="row" onsubmit={create}>
    <input
      type="text"
      bind:value={name}
      maxlength="120"
      required
      placeholder={$t('projects.new_name')}
    />
    <button type="submit">{$t('projects.create')}</button>
  </form>
  {#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
  {#if items.length === 0}
    <p class="muted">{$t('projects.empty')}</p>
  {:else}
    <ul class="list">
      {#each items as item (item.id)}
        <li>
          <a href="#/projects/{item.id}">{item.name}</a>
          <span class="muted">· {$t('projects.count', { count: item.blueprints })}</span>
          <button class="secondary" onclick={() => remove(item)}>{$t('common.delete')}</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
