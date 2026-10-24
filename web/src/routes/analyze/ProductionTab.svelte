<!--
  PT: Aba de produção: saldo por item no regime estável (com o nominal a 100% ao lado), gargalos e
      uso de cada grupo de receita.
  EN: Production tab: per-item balance in steady state (with the 100% nominal next to it),
      bottlenecks and use of each recipe group.
-->
<script lang="ts">
  import ItemIcon from '../../lib/components/ItemIcon.svelte';
  import { formatPercent, formatRate, type RateUnit } from '../../lib/format';
  import { language, t } from '../../lib/i18n';
  import { gameLocale } from '../../lib/game/store';
  import { recipeName } from '../../lib/game/assets';
  import type { CalcReport } from '../../lib/types';

  let { report, unit }: { report: CalcReport; unit: RateUnit } = $props();

  const nominal = $derived(new Map(report.nominal.map((flow) => [flow.name, flow])));
  const rows = $derived(
    [...report.steady].sort(
      (a, b) =>
        b.produced_per_second -
        b.consumed_per_second -
        (a.produced_per_second - a.consumed_per_second),
    ),
  );
  const bottlenecks = $derived(new Set(report.steady_state.bottlenecks));
  const name = (item: string) => recipeName($gameLocale, item);
</script>

{#if rows.length === 0}
  <p class="muted">{$t('production.empty')}</p>
{:else}
  {#if !report.steady_state.solved}
    <p class="warning">{$t('production.not_solved')}</p>
  {/if}
  <table>
    <thead>
      <tr>
        <th>{$t('production.item')}</th>
        <th class="num">{$t('production.produced')}</th>
        <th class="num">{$t('production.consumed')}</th>
        <th class="num">{$t('production.net')}</th>
        <th class="num">{$t('production.nominal_net')}</th>
      </tr>
    </thead>
    <tbody>
      {#each rows as flow (flow.name)}
        {@const net = flow.produced_per_second - flow.consumed_per_second}
        {@const full = nominal.get(flow.name)}
        <tr class:bottleneck={bottlenecks.has(flow.name)}>
          <td>
            <span class="item">
              <ItemIcon kind="item" name={flow.name} title={name(flow.name)} />
              <span>{name(flow.name)}</span>
              {#if bottlenecks.has(flow.name)}<span class="badge"
                  >{$t('production.bottleneck')}</span
                >{/if}
            </span>
          </td>
          <td class="num">{formatRate($language, flow.produced_per_second, unit)}</td>
          <td class="num">{formatRate($language, flow.consumed_per_second, unit)}</td>
          <td class="num" class:positive={net > 1e-9} class:negative={net < -1e-9}>
            {formatRate($language, net, unit)}
          </td>
          <td class="num muted">
            {full
              ? formatRate($language, full.produced_per_second - full.consumed_per_second, unit)
              : '—'}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  <h3>{$t('production.groups')}</h3>
  <table>
    <thead>
      <tr>
        <th>{$t('production.recipe')}</th>
        <th class="num">{$t('production.machines')}</th>
        <th class="num">{$t('production.utilization')}</th>
      </tr>
    </thead>
    <tbody>
      {#each report.groups as group (group.recipe)}
        {@const use = report.steady_state.utilization[group.recipe] ?? 1}
        <tr>
          <td>
            <span class="item">
              <ItemIcon kind="recipe" name={group.recipe} title={name(group.recipe)} />
              <span>{name(group.recipe)}</span>
            </span>
          </td>
          <td class="num">{group.machines}</td>
          <td class="num" class:negative={use < 0.999}>{formatPercent($language, use)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    text-align: left;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .item {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .positive {
    color: var(--positive);
  }
  .negative {
    color: var(--error);
  }
  .badge {
    font-size: 11px;
    background: var(--error);
    color: #fff;
    border-radius: 4px;
    padding: 0 5px;
  }
  .muted {
    color: var(--muted);
  }
  .warning {
    color: var(--warning);
  }
  h3 {
    font-size: 14px;
    margin: 16px 0 6px;
  }
</style>
