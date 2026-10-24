<!--
  PT: Aba de materiais: itens necessários para construir a blueprint.
  EN: Materials tab: items needed to build the blueprint.
-->
<script lang="ts">
  import ItemIcon from '../../lib/components/ItemIcon.svelte';
  import { recipeName } from '../../lib/game/assets';
  import { gameLocale } from '../../lib/game/store';
  import { language, t } from '../../lib/i18n';
  import type { CalcReport } from '../../lib/types';

  let { report }: { report: CalcReport } = $props();

  const lines = $derived([...report.bill_of_materials].sort((a, b) => b.count - a.count));
</script>

<table>
  <thead>
    <tr>
      <th>{$t('materials.item')}</th>
      <th class="num">{$t('materials.count')}</th>
    </tr>
  </thead>
  <tbody>
    {#each lines as line (line.item)}
      <tr>
        <td>
          <span class="item">
            <ItemIcon kind="item" name={line.item} />
            <span>{recipeName($gameLocale, line.item)}</span>
          </span>
        </td>
        <td class="num">{new Intl.NumberFormat($language).format(line.count)}</td>
      </tr>
    {/each}
  </tbody>
</table>

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
  }
  .item {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
</style>
