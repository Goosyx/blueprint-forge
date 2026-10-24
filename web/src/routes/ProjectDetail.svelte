<!--
  PT: Detalhe de um projeto: blueprints dele, com opção de remover.
  EN: Project detail: its blueprints, with an option to remove them.
-->
<script lang="ts">
  import { api, type ProjectDetail } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { t } from '../lib/i18n';

  let { id }: { id: string } = $props();
  let detail = $state<ProjectDetail | null>(null);
  let error = $state<unknown>(null);

  async function load(current: string): Promise<void> {
    try {
      detail = await api.get<ProjectDetail>(`/projects/${current}`);
    } catch (failure) {
      error = failure;
    }
  }

  $effect(() => {
    void load(id);
  });

  async function remove(blueprint: string): Promise<void> {
    try {
      await api.delete(`/projects/${id}/blueprints/${blueprint}`);
      await load(id);
    } catch (failure) {
      error = failure;
    }
  }
</script>

{#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
{#if detail}
  <section class="card">
    <h2>{detail.name}</h2>
    {#if detail.description}<p class="description">{detail.description}</p>{/if}
    {#if detail.items.length === 0}
      <p class="muted">{$t('projects.no_blueprints')}</p>
    {:else}
      <ul class="list">
        {#each detail.items as item (item.id)}
          <li>
            <a href="#/blueprints/{item.id}">{item.title}</a>
            <button class="secondary" onclick={() => remove(item.id)}
              >{$t('projects.remove')}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}
