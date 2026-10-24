<!--
  PT: Minhas blueprints: lista paginada, abrir, analisar e apagar.
  EN: My blueprints: paginated list, open, analyze and delete.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type BlueprintItem } from '../lib/api/client';
  import { errorText } from '../lib/errors';
  import { language, t } from '../lib/i18n';

  let items = $state<BlueprintItem[]>([]);
  let more = $state(false);
  let busy = $state(false);
  let error = $state<unknown>(null);
  const PAGE = 50;

  async function load(after?: string): Promise<void> {
    busy = true;
    error = null;
    try {
      const query = `?limit=${PAGE}${after ? `&after=${encodeURIComponent(after)}` : ''}`;
      const page = await api.get<BlueprintItem[]>(`/blueprints${query}`);
      items = after ? [...items, ...page] : page;
      more = page.length === PAGE;
    } catch (failure) {
      error = failure;
    } finally {
      busy = false;
    }
  }

  async function remove(item: BlueprintItem): Promise<void> {
    if (!confirm($t('blueprints.confirm_delete', { title: item.title }))) return;
    try {
      await api.delete(`/blueprints/${item.id}`);
      items = items.filter((other) => other.id !== item.id);
    } catch (failure) {
      error = failure;
    }
  }

  onMount(() => void load());
</script>

<section class="card">
  <h2>{$t('blueprints.title')}</h2>
  {#if error}<p class="error" role="alert">{errorText($t, error)}</p>{/if}
  {#if items.length === 0 && !busy}
    <p class="muted">{$t('blueprints.empty')}</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>{$t('blueprints.name')}</th>
          <th class="num">{$t('blueprints.entities')}</th>
          <th>{$t('blueprints.created')}</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each items as item (item.id)}
          <tr>
            <td>
              <a href="#/blueprints/{item.id}">{item.title}</a>
              {#if item.is_book}<span class="muted">
                  · {$t('blueprints.book', { count: item.blueprint_count })}</span
                >{/if}
            </td>
            <td class="num">{new Intl.NumberFormat($language).format(item.entity_count)}</td>
            <td>{new Date(item.created_at).toLocaleString($language)}</td>
            <td class="actions"
              ><button class="secondary" onclick={() => remove(item)}>{$t('common.delete')}</button
              ></td
            >
          </tr>
        {/each}
      </tbody>
    </table>
    {#if more}
      <button class="secondary" disabled={busy} onclick={() => load(items.at(-1)?.id)}
        >{$t('common.load_more')}</button
      >
    {/if}
  {/if}
</section>
