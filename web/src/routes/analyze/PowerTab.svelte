<!--
  PT: Aba de energia: consumo médio, pico e dreno por categoria; geração disponível e poluição.
  EN: Power tab: average, peak and drain consumption by category; available generation and pollution.
-->
<script lang="ts">
  import {
    formatEnergy,
    formatPercent,
    formatPower,
    formatRate,
    type RateUnit,
  } from '../../lib/format';
  import { language, t } from '../../lib/i18n';
  import type { CalcReport } from '../../lib/types';

  let { report, unit }: { report: CalcReport; unit: RateUnit } = $props();

  const power = $derived(report.power);
  const generation = $derived(report.generation);
  const hasGeneration = $derived(
    generation.electric_capacity_w > 0 ||
      generation.accumulator_energy_j > 0 ||
      generation.reactor_heat_w > 0,
  );
  const satisfaction = $derived(
    power.average_w > 0 ? generation.electric_capacity_w / power.average_w : null,
  );
</script>

<section class="cards">
  <div class="card">
    <span class="label">{$t('power.average')}</span>
    <strong>{formatPower($language, power.average_w)}</strong>
  </div>
  <div class="card">
    <span class="label">{$t('power.peak')}</span>
    <strong>{formatPower($language, power.peak_w)}</strong>
  </div>
  <div class="card">
    <span class="label">{$t('power.drain')}</span>
    <strong>{formatPower($language, power.drain_w)}</strong>
  </div>
  <div class="card">
    <span class="label">{$t('power.pollution')}</span>
    <strong>{formatRate($language, report.pollution_per_minute / 60, unit)}</strong>
  </div>
</section>

<h3>{$t('power.by_category')}</h3>
<table>
  <tbody>
    {#each Object.entries(power.by_category) as [category, watts] (category)}
      <tr>
        <td>{$t(`power.category.${category}`)}</td>
        <td class="num">{formatPower($language, watts)}</td>
      </tr>
    {/each}
  </tbody>
</table>
<p class="muted note">{$t('power.inserter_note')}</p>

{#if hasGeneration}
  <h3>{$t('power.generation')}</h3>
  <table>
    <tbody>
      <tr
        ><td>{$t('power.capacity')}</td><td class="num"
          >{formatPower($language, generation.electric_capacity_w)}</td
        ></tr
      >
      <tr
        ><td>{$t('power.steam')}</td><td class="num"
          >{formatPower($language, generation.steam_electric_w)}</td
        ></tr
      >
      <tr
        ><td>{$t('power.solar_peak')}</td><td class="num"
          >{formatPower($language, generation.solar_peak_w)}</td
        ></tr
      >
      <tr
        ><td>{$t('power.solar_average')}</td><td class="num"
          >{formatPower($language, generation.solar_average_w)}</td
        ></tr
      >
      <tr
        ><td>{$t('power.accumulators')}</td><td class="num"
          >{formatEnergy($language, generation.accumulator_energy_j)}</td
        ></tr
      >
      {#if generation.reactor_heat_w > 0}
        <tr
          ><td>{$t('power.reactor_heat')}</td><td class="num"
            >{formatPower($language, generation.reactor_heat_w)}</td
          ></tr
        >
      {/if}
      {#if generation.boiler_fuel_per_second > 0}
        <tr
          ><td>{$t('power.boiler_fuel')}</td><td class="num"
            >{formatRate($language, generation.boiler_fuel_per_second, unit)}</td
          ></tr
        >
      {/if}
      {#if satisfaction !== null}
        <tr
          ><td>{$t('power.satisfaction')}</td><td class="num"
            >{formatPercent($language, satisfaction)}</td
          ></tr
        >
      {/if}
    </tbody>
  </table>
{/if}

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
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
  td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  h3 {
    font-size: 14px;
    margin: 16px 0 6px;
  }
  .muted {
    color: var(--muted);
  }
  .note {
    font-size: 12px;
  }
</style>
