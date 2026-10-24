<!--
  PT: Aba de máquinas: receita, módulos, beacons e efeitos de cada máquina. Máquinas sem receita
      (fornalhas) ganham uma lista para escolher a receita, que vai para as opções da análise.
  EN: Machines tab: recipe, modules, beacons and effects of each machine. Machines without a recipe
      (furnaces) get a list to choose the recipe, which goes into the analysis options.
-->
<script lang="ts">
  import ItemIcon from '../../lib/components/ItemIcon.svelte';
  import { entityName, recipeName } from '../../lib/game/assets';
  import { gameIndex, gameLocale } from '../../lib/game/store';
  import { formatPercent, formatPower, formatRate, type RateUnit } from '../../lib/format';
  import { language, t } from '../../lib/i18n';
  import type { CalcReport, MachineReport } from '../../lib/types';

  let {
    report,
    unit,
    onRecipe,
  }: {
    report: CalcReport;
    unit: RateUnit;
    onRecipe: (entityNumber: number, recipe: string | null) => void;
  } = $props();

  // PT: Receitas possíveis por tipo de máquina, calculadas uma vez por índice (não por linha a
  //     cada renderização). Uma receita pode estar em mais de uma categoria da máquina (2.1+);
  //     o Set tira as repetidas.
  // EN: Possible recipes per machine type, computed once per index (not per row on every
  //     render). A recipe can be in more than one of the machine's categories (2.1+); the Set
  //     drops repeats.
  const optionsByMachine = $derived.by(() => {
    const index = $gameIndex;
    const result: Record<string, string[]> = {};
    if (!index) return result;
    for (const [name, categories] of Object.entries(index.machineCategories)) {
      const names = new Set(
        categories.flatMap((category) => index.recipesByCategory[category] ?? []),
      );
      result[name] = [...names].sort();
    }
    return result;
  });

  function options(machine: MachineReport): string[] {
    return optionsByMachine[machine.name] ?? [];
  }
</script>

<table>
  <thead>
    <tr>
      <th>#</th>
      <th>{$t('machines.machine')}</th>
      <th>{$t('machines.recipe')}</th>
      <th class="num">{$t('machines.speed')}</th>
      <th class="num">{$t('machines.productivity')}</th>
      <th class="num">{$t('machines.rate')}</th>
      <th class="num">{$t('machines.power')}</th>
    </tr>
  </thead>
  <tbody>
    {#each report.machines as machine (machine.entity_number)}
      <tr>
        <td class="muted">{machine.entity_number}</td>
        <td class="item">
          <ItemIcon
            kind="entity"
            name={machine.name}
            title={entityName($gameLocale, machine.name)}
          />
          <span>{entityName($gameLocale, machine.name)}</span>
          {#if machine.beacons}<span class="muted"
              >· {$t('machines.beacons', { count: machine.beacons })}</span
            >{/if}
        </td>
        <td>
          {#if machine.recipe && (machine.recipe_source === 'blueprint' || machine.recipe_source === 'fixed')}
            <span class="item">
              <ItemIcon kind="recipe" name={machine.recipe} />
              {recipeName($gameLocale, machine.recipe)}
            </span>
          {:else}
            <select
              aria-label={$t('machines.choose_recipe')}
              value={machine.recipe ?? ''}
              onchange={(event) =>
                onRecipe(machine.entity_number, event.currentTarget.value || null)}
            >
              <option value="">{$t('machines.no_recipe')}</option>
              {#each options(machine) as recipe (recipe)}
                <option value={recipe}>{recipeName($gameLocale, recipe)}</option>
              {/each}
            </select>
            {#if machine.recipe_source === 'inferred'}<span
                class="badge"
                title={$t('machines.inferred_hint')}>{$t('machines.inferred')}</span
              >{/if}
          {/if}
        </td>
        <td class="num">{formatPercent($language, machine.effect.speed)}</td>
        <td class="num">{formatPercent($language, machine.effect.productivity)}</td>
        <td class="num">{formatRate($language, machine.crafts_per_second, unit)}</td>
        <td class="num">{formatPower($language, machine.power_w + machine.drain_w)}</td>
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
    white-space: nowrap;
  }
  .item {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .muted {
    color: var(--muted);
  }
  .badge {
    margin-left: 6px;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
