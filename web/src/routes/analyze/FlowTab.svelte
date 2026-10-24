<!--
  PT: Aba de fluxo: eficiência geral, a causa de cada máquina abaixo do máximo (falta de insumo,
      insersor lento, faixa saturada, saída bloqueada...) pela simulação em regime estável, o nível
      de pesquisa dos insersores e a simulação tick a tick sob demanda, comparada com a estável.
  EN: Flow tab: overall efficiency, the cause of each machine below its maximum (missing input,
      slow inserter, saturated lane, blocked output...) from the steady-state simulation, the
      inserter research level and the on-demand tick simulation, compared with the steady one.
-->
<script lang="ts">
  import ItemIcon from '../../lib/components/ItemIcon.svelte';
  import { entityName, recipeName } from '../../lib/game/assets';
  import { gameLocale } from '../../lib/game/store';
  import { formatPercent, formatRate, type RateUnit } from '../../lib/format';
  import { language, t } from '../../lib/i18n';
  import type { Analysis, Bottleneck, Cause, FlowReport } from '../../lib/types';

  let {
    analysis,
    unit,
    research,
    busy,
    onResearch,
    tick,
    tickState,
    onSimulate,
    onCancel,
  }: {
    analysis: Analysis;
    unit: RateUnit;
    research: 'none' | 'all';
    busy: boolean;
    onResearch: (level: 'none' | 'all') => void;
    tick: FlowReport | null;
    tickState: 'idle' | 'running' | 'failed';
    onSimulate: () => void;
    onCancel: () => void;
  } = $props();

  const names = $derived(
    new Map(analysis.report.machines.map((m) => [m.entity_number, m.name] as const)),
  );
  const efficiency = $derived(analysis.efficiency);
  const problems = $derived(
    (efficiency?.machines ?? [])
      .filter((m) => m.cause.type !== 'ok')
      .sort((a, b) => a.utilization - b.utilization),
  );
  const items = $derived(
    [
      ...new Set([
        ...Object.keys(analysis.flow?.produced ?? {}),
        ...Object.keys(tick?.produced ?? {}),
      ]),
    ].sort(),
  );

  function causeText(cause: Cause): string {
    const item = 'item' in cause ? recipeName($gameLocale, cause.item) : '';
    if (cause.type === 'slow_inserter') {
      return $t('flow.cause.slow_inserter', { item, inserter: cause.inserter });
    }
    return $t(`flow.cause.${cause.type}`, { item });
  }

  function machineName(machine: Bottleneck): string {
    return entityName($gameLocale, names.get(machine.entity_number) ?? '');
  }
</script>

{#if !analysis.flow}
  <p class="muted">{$t('flow.unavailable')}</p>
{:else}
  <section class="cards">
    <div class="card">
      <span class="label">{$t('flow.efficiency')}</span>
      <strong>{formatPercent($language, efficiency?.efficiency ?? 0)}</strong>
    </div>
    <div class="card">
      <span class="label">{$t('flow.limited')}</span>
      <strong>{problems.length}</strong>
    </div>
    <label class="card">
      <span class="label">{$t('flow.research')}</span>
      <select
        value={research}
        disabled={busy}
        onchange={(event) => onResearch(event.currentTarget.value === 'none' ? 'none' : 'all')}
      >
        <option value="all">{$t('flow.research_all')}</option>
        <option value="none">{$t('flow.research_none')}</option>
      </select>
    </label>
  </section>

  <h3>{$t('flow.bottlenecks')}</h3>
  {#if problems.length === 0}
    <p class="muted">{$t('flow.all_ok')}</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>{$t('machines.machine')}</th>
          <th class="num">{$t('flow.utilization')}</th>
          <th>{$t('flow.cause')}</th>
        </tr>
      </thead>
      <tbody>
        {#each problems as machine (machine.entity_number)}
          <tr>
            <td class="muted">{machine.entity_number}</td>
            <td class="name">
              <ItemIcon
                kind="item"
                name={names.get(machine.entity_number) ?? ''}
                title={machineName(machine)}
              />
              <span>{machineName(machine)}</span>
            </td>
            <td class="num">{formatPercent($language, machine.utilization)}</td>
            <td>{causeText(machine.cause)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <h3>{$t('flow.compare')}</h3>
  <p class="muted note">{$t('flow.tick_note')}</p>
  <div class="actions">
    {#if tickState === 'running'}
      <span class="muted">{$t('flow.tick_running')}</span>
      <button type="button" class="secondary" onclick={onCancel}>{$t('flow.cancel')}</button>
    {:else}
      <button type="button" onclick={onSimulate}>{$t('flow.tick_run')}</button>
      {#if tickState === 'failed'}<span class="error">{$t('flow.tick_failed')}</span>{/if}
    {/if}
  </div>
  <table>
    <thead>
      <tr>
        <th>{$t('flow.item')}</th>
        <th class="num">{$t('flow.steady')}</th>
        <th class="num">{$t('flow.tick')}</th>
      </tr>
    </thead>
    <tbody>
      {#each items as item (item)}
        <tr>
          <td class="name">
            <ItemIcon kind="item" name={item} title={recipeName($gameLocale, item)} />
            <span>{recipeName($gameLocale, item)}</span>
          </td>
          <td class="num">{formatRate($language, analysis.flow.produced[item] ?? 0, unit)}</td>
          <td class="num">{tick ? formatRate($language, tick.produced[item] ?? 0, unit) : '—'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 8px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    color: var(--muted);
    font-size: 12px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
  }
  .note {
    font-size: 12px;
  }
  .error {
    color: var(--error);
  }
</style>
