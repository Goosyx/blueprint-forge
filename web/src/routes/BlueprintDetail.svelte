<!--
  PT: Detalhe de uma blueprint salva: editar nome e descrição, abrir na análise, copiar a string,
      ver os resumos das análises do servidor e adicionar a um projeto.
  EN: Saved blueprint detail: edit name and description, open in the analysis, copy the string,
      see the server analysis summaries and add it to a project.
-->
<script lang="ts">
  import { api, type BlueprintDetail, type Project } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { formatPower, formatRate } from '../lib/format';
  import { recipeName } from '../lib/game/assets';
  import { gameLocale } from '../lib/game/store';
  import { language, t } from '../lib/i18n';
  import { go } from '../lib/router';
  import { pendingSource } from '../lib/session';

  let { id }: { id: string } = $props();

  let detail = $state<BlueprintDetail | null>(null);
  let projects = $state<Project[]>([]);
  let title = $state('');
  let description = $state('');
  let project = $state('');
  let message = $state<string | null>(null);
  let error = $state<unknown>(null);

  $effect(() => {
    const current = id;
    error = null;
    detail = null;
    api
      .get<BlueprintDetail>(`/blueprints/${current}`)
      .then((loaded) => {
        // PT: Descarta a resposta se o usuário já trocou de blueprint. EN: Drops the response if the user already switched blueprints.
        if (current !== id) return;
        detail = loaded;
        title = loaded.title;
        description = loaded.description ?? '';
      })
      .catch((failure) => {
        if (current === id) error = failure;
      });
    api
      .get<Project[]>('/projects?limit=100')
      .then((list) => (projects = list))
      .catch(() => (projects = []));
  });

  async function save(): Promise<void> {
    if (!detail || detail.id !== id) return;
    error = null;
    try {
      await api.patch(`/blueprints/${detail.id}`, { title, description });
      message = $t('common.saved');
    } catch (failure) {
      error = failure;
    }
  }

  async function addToProject(): Promise<void> {
    if (!project) return;
    error = null;
    try {
      await api.post(`/projects/${project}/blueprints`, { blueprint_id: id });
      message = $t('blueprint.added_to_project');
    } catch (failure) {
      error = failure;
    }
  }

  async function copy(): Promise<void> {
    if (!detail) return;
    try {
      await navigator.clipboard.writeText(detail.source);
      message = $t('blueprint.copied');
    } catch {
      message = null;
    }
  }

  function openAnalysis(): void {
    if (!detail) return;
    pendingSource.set(detail.source);
    go('/');
  }
</script>

{#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
{#if detail}
  <section class="card form wide">
    <label>
      <span>{$t('blueprint.name')}</span>
      <input type="text" bind:value={title} maxlength="200" />
    </label>
    <label>
      <span>{$t('blueprint.description')}</span>
      <textarea bind:value={description} maxlength="2000" rows="3"></textarea>
    </label>
    <div class="row">
      <button onclick={save}>{$t('common.save')}</button>
      <button class="secondary" onclick={openAnalysis}>{$t('blueprint.open_analysis')}</button>
      <button class="secondary" onclick={copy}>{$t('blueprint.copy')}</button>
    </div>
    <div class="row">
      <select bind:value={project} aria-label={$t('blueprint.project')}>
        <option value="">{$t('blueprint.choose_project')}</option>
        {#each projects as item (item.id)}
          <option value={item.id}>{item.name}</option>
        {/each}
      </select>
      <button class="secondary" disabled={!project} onclick={addToProject}
        >{$t('blueprint.add_to_project')}</button
      >
    </div>
    {#if message}<p class="muted">{message}</p>{/if}
  </section>

  <section class="card">
    <h3>{$t('blueprint.analyses')}</h3>
    {#each detail.analyses as saved (saved.id)}
      <div class="analysis">
        <p>
          {$t('analyze.entities', { count: saved.summary.selected_entities })} ·
          {$t('analyze.power_summary', {
            power: formatPower($language, saved.summary.power_average_w),
          })}
        </p>
        <ul>
          {#each saved.summary.outputs as [name, rate] (name)}
            <li>{recipeName($gameLocale, name)}: {formatRate($language, rate, 'minute')}</li>
          {/each}
        </ul>
        <p class="muted">
          {new Date(saved.created_at).toLocaleString($language)} · {saved.data_version}
        </p>
      </div>
    {/each}
  </section>
{/if}
