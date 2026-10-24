<!--
  PT: Página "Analisar": colar a blueprint string, analisar no Web Worker e mostrar o desenho e as
      abas de produção, energia, máquinas e materiais. Texto vindo da blueprint é exibido só como texto.
  EN: "Analyze" page: paste the blueprint string, analyze it in the Web Worker and show the drawing
      and the production, power, machines and materials tabs. Text from the blueprint is shown as text only.
-->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { errorKey, language, t } from '../lib/i18n';
  import {
    atlasPageUrl,
    entityName,
    fetchAtlas,
    fetchGameData,
    fetchLocale,
    fetchVersions,
    recipeName,
  } from '../lib/game/assets';
  import { atlasState, buildGameIndex, gameIndex, gameLocale } from '../lib/game/store';
  import { EngineClient, EngineError } from '../lib/wasm/client';
  import { BlueprintScene, type HoverInfo } from '../lib/render/scene';
  import { formatPower, formatRate, type RateUnit } from '../lib/format';
  import type {
    Analysis,
    AnalysisOptions,
    CalcWarning,
    FlowReport,
    TreeNode,
    WorldWarning,
  } from '../lib/types';
  import BlueprintHeader from '../lib/components/BlueprintHeader.svelte';
  import ProductionTab from './analyze/ProductionTab.svelte';
  import { api } from '../lib/api/client';
  import { errorText as apiErrorText } from '../lib/errors';
  import { go } from '../lib/router';
  import { me, pendingSource } from '../lib/session';
  import PowerTab from './analyze/PowerTab.svelte';
  import MachinesTab from './analyze/MachinesTab.svelte';
  import MaterialsTab from './analyze/MaterialsTab.svelte';
  import FlowTab from './analyze/FlowTab.svelte';
  import SankeyTab from './analyze/SankeyTab.svelte';

  type Tab = 'drawing' | 'production' | 'flow' | 'sankey' | 'power' | 'machines' | 'materials';
  const tabs: Tab[] = ['drawing', 'production', 'flow', 'sankey', 'power', 'machines', 'materials'];
  /** PT: Aquecimento e medição da simulação por tick, em ticks. EN: Tick simulation warm-up and measurement, in ticks. */
  const TICK_WARMUP = 36_000;
  const TICK_MEASURE = 7_200;

  let input = $state('');
  let analyzedText = $state('');
  let engineState = $state<'loading' | 'ready' | 'failed'>('loading');
  let working = $state(false);
  let errorCode = $state<string | null>(null);
  let analysis = $state<Analysis | null>(null);
  let selected = $state(0);
  let recipeOverrides = $state<Record<string, string>>({});
  let tab = $state<Tab>('drawing');
  let unit = $state<RateUnit>('minute');
  let hover = $state<HoverInfo | null>(null);
  let hasIcons = $state(true);
  let gameVersion = $state('');
  let canvasHost: HTMLDivElement;
  let saveMessage = $state<string | null>(null);
  let saving = $state(false);
  let research = $state<'none' | 'all'>('all');
  let overlay = $state(false);
  let tick = $state<FlowReport | null>(null);
  let tickState = $state<'idle' | 'running' | 'failed'>('idle');
  // PT: Identifica a simulação por tick em curso; resultado ou erro de outra é descartado.
  // EN: Identifies the running tick simulation; another one's result or error is dropped.
  let tickToken = 0;

  const engine = new EngineClient();
  let scene: BlueprintScene | null = null;

  const current = $derived(analysis?.blueprints[selected] ?? null);

  onMount(async () => {
    scene = new BlueprintScene(canvasHost, (info) => (hover = info));
    try {
      await scene.init();
      const versions = await fetchVersions();
      const version = versions[0];
      if (!version) throw new Error('no game version');
      gameVersion = version.version;
      const data = await fetchGameData(gameVersion);
      gameIndex.set(buildGameIndex(data));
      await engine.start(data);
      engineState = 'ready';
      const atlas = await fetchAtlas(gameVersion);
      hasIcons = atlas !== null;
      if (atlas) {
        const pageUrl = (page: string) => atlasPageUrl(gameVersion, page);
        atlasState.set({ atlas, pageUrl });
        await scene.setAtlas(atlas, pageUrl);
      }
    } catch {
      engineState = 'failed';
    }
  });

  onDestroy(() => {
    engine.stop();
    scene?.destroy();
  });

  // PT: Recarrega os nomes do jogo quando o idioma muda e descarta respostas atrasadas.
  // EN: Reloads game names when the language changes and drops late responses.
  $effect(() => {
    const lang = $language;
    if (!gameVersion) return;
    fetchLocale(gameVersion, lang)
      .then((locale) => {
        if (lang === $language) gameLocale.set(locale);
      })
      .catch(() => gameLocale.set(null));
  });

  $effect(() => {
    if (analysis && tab === 'drawing') scene?.show(analysis.world);
  });

  // PT: Cores de utilização no desenho, pela simulação em regime estável.
  // EN: Utilization colors on the drawing, from the steady-state simulation.
  $effect(() => {
    const flow = analysis?.flow;
    scene?.setOverlay(
      overlay && flow
        ? new Map(flow.machines.map((m) => [m.entity_number, m.utilization] as const))
        : null,
    );
  });

  // PT: Abre um texto vindo de outra página (detalhe da blueprint) assim que o motor fica pronto.
  // EN: Opens text coming from another page (blueprint detail) as soon as the engine is ready.
  $effect(() => {
    const pending = $pendingSource;
    // PT: Só consome o texto pendente com o motor livre; senão espera a análise atual terminar.
    // EN: Only consumes the pending text with the engine idle; otherwise waits for the current analysis.
    if (pending && engineState === 'ready' && !working) {
      pendingSource.set(null);
      input = pending;
      analyzeInput();
    }
  });

  // PT: Ao sair da conta, limpa a análise para o próximo usuário não ver a blueprint do anterior.
  // EN: On sign-out, clears the analysis so the next user does not see the previous one's blueprint.
  let wasLoggedIn = false;
  $effect(() => {
    const loggedIn = $me !== null;
    if (wasLoggedIn && !loggedIn) clear();
    wasLoggedIn = loggedIn;
  });

  async function saveBlueprint(): Promise<void> {
    if (!analyzedText || saving) return;
    saving = true;
    saveMessage = null;
    try {
      const saved = await api.post<{ id: string }>('/blueprints', { source: analyzedText });
      go(`/blueprints/${saved.id}`);
    } catch (failure) {
      saveMessage = apiErrorText($t, failure);
    } finally {
      saving = false;
    }
  }

  async function run(text: string, index: number): Promise<void> {
    if (engineState !== 'ready' || working) return;
    // PT: A análise e a simulação dividem o worker: a simulação em curso é cancelada antes.
    // EN: The analysis and the simulation share the worker: a running simulation is cancelled first.
    if (tickState === 'running') cancelTicks();
    working = true;
    errorCode = null;
    const options: AnalysisOptions = {
      recipe_overrides: recipeOverrides,
      inserter_research: research,
    };
    tickToken++;
    tick = null;
    tickState = 'idle';
    try {
      analysis = await engine.analyze(text, index, options);
      analyzedText = text;
      selected = index;
    } catch (error) {
      errorCode = error instanceof EngineError ? error.code : 'internal';
      if (text !== analyzedText) analysis = null;
    } finally {
      working = false;
    }
  }

  function analyzeInput(): void {
    recipeOverrides = {};
    void run(input, 0);
  }

  function select(index: number): void {
    recipeOverrides = {};
    void run(analyzedText, index);
  }

  function setRecipe(entityNumber: number, recipe: string | null): void {
    const next = { ...recipeOverrides };
    if (recipe) next[String(entityNumber)] = recipe;
    else delete next[String(entityNumber)];
    recipeOverrides = next;
    void run(analyzedText, selected);
  }

  function setResearch(level: 'none' | 'all'): void {
    if (working) return;
    research = level;
    void run(analyzedText, selected);
  }

  async function simulateTicks(): Promise<void> {
    if (!analyzedText || tickState === 'running' || working) return;
    const token = ++tickToken;
    tickState = 'running';
    try {
      const report = await engine.simulate(
        analyzedText,
        selected,
        { recipe_overrides: recipeOverrides, inserter_research: research },
        TICK_WARMUP,
        TICK_MEASURE,
      );
      if (token === tickToken) {
        tick = report;
        tickState = 'idle';
      }
    } catch {
      if (token === tickToken) tickState = 'failed';
    }
  }

  function cancelTicks(): void {
    tickToken++;
    engine.cancel();
    tickState = 'idle';
  }

  function clear(): void {
    input = '';
    analyzedText = '';
    analysis = null;
    errorCode = null;
    recipeOverrides = {};
    tick = null;
    tickState = 'idle';
  }

  function worldWarningText(warning: WorldWarning): string {
    if (warning.code === 'unknown_entity') {
      return $t('warning.unknown_entity', {
        name: warning.name,
        entity_number: warning.entity_number,
      });
    }
    return $t('warning.overlap', {
      first: warning.first,
      second: warning.second,
      x: warning.x,
      y: warning.y,
    });
  }

  function calcWarningText(warning: CalcWarning): string {
    if (warning.code === 'no_recipe' || warning.code === 'unpowered')
      return $t(`warning.${warning.code}`, { entity_number: warning.entity_number });
    return $t(`warning.${warning.code}`, {
      entity_number: warning.entity_number,
      recipe: warning.recipe,
    });
  }

  function errorText(code: string): string {
    const key = errorKey(code);
    const text = $t(key);
    return text === key ? $t('error.generic', { code }) : text;
  }

  // PT: Saídas principais: itens com saldo positivo no regime estável. EN: Main outputs: items with a positive steady balance.
  const outputs = $derived(
    (analysis?.report.steady ?? [])
      .map((flow) => ({
        name: flow.name,
        net: flow.produced_per_second - flow.consumed_per_second,
      }))
      // PT: Ignora resíduos numéricos do solver. EN: Ignores the solver's numeric residue.
      .filter((flow) => flow.net > 1e-6)
      .sort((a, b) => b.net - a.net)
      .slice(0, 6),
  );
</script>

{#snippet treeNode(node: TreeNode, depth: number)}
  {#if node.type === 'book'}
    <li class="book" style:padding-left="{depth * 12}px">
      <span class="book-label">📘 {node.label ?? $t('analyze.book')}</span>
      <ul>
        {#each node.children as child, i (i)}
          {@render treeNode(child, depth + 1)}
        {/each}
      </ul>
    </li>
  {:else if node.type === 'blueprint'}
    {@const bp = analysis?.blueprints[node.blueprint]}
    <li style:padding-left="{depth * 12}px">
      <button
        class="tree-item"
        class:active={selected === node.blueprint}
        disabled={working}
        onclick={() => select(node.blueprint)}
      >
        {bp?.label || $t('analyze.untitled')}
      </button>
    </li>
  {:else}
    <li class="muted" style:padding-left="{depth * 12}px">{$t('analyze.other_entry')}</li>
  {/if}
{/snippet}

<section class="analyze">
  <form
    class="input"
    onsubmit={(event) => {
      event.preventDefault();
      analyzeInput();
    }}
  >
    <label for="blueprint-input">{$t('analyze.input_label')}</label>
    <textarea
      id="blueprint-input"
      bind:value={input}
      placeholder={$t('analyze.placeholder')}
      spellcheck="false"
      autocomplete="off"
      rows="3"></textarea>
    <div class="actions">
      <button type="submit" disabled={engineState !== 'ready' || working || !input.trim()}>
        {working ? $t('analyze.working') : $t('analyze.submit')}
      </button>
      <button type="button" class="secondary" onclick={clear}>{$t('analyze.clear')}</button>
      {#if $me && analysis}
        <button type="button" class="secondary" disabled={saving} onclick={saveBlueprint}
          >{$t('analyze.save')}</button
        >
      {/if}
      <label class="unit">
        <span>{$t('analyze.unit')}</span>
        <select bind:value={unit}>
          <option value="second">/s</option>
          <option value="minute">/min</option>
          <option value="hour">/h</option>
        </select>
      </label>
      {#if engineState === 'loading'}<span class="muted">{$t('analyze.loading_engine')}</span>{/if}
      {#if engineState === 'failed'}<span class="error">{$t('analyze.engine_failed')}</span>{/if}
    </div>
    {#if errorCode}<p class="error" role="alert">{errorText(errorCode)}</p>{/if}
    {#if saveMessage}<p class="error" role="alert">{saveMessage}</p>{/if}
  </form>

  <div class="workspace">
    <aside class="sidebar">
      {#if analysis}
        {#if analysis.tree.type === 'book'}
          <ul class="tree">{@render treeNode(analysis.tree, 0)}</ul>
        {/if}
        {#if current}
          <BlueprintHeader
            label={current.label}
            description={current.description}
            untitled={$t('analyze.untitled')}
          />
          <p>{$t('analyze.entities', { count: current.entity_count })}</p>
          {#if analysis.world.bounds}
            <p>
              {$t('analyze.size', {
                width: analysis.world.bounds.max_x - analysis.world.bounds.min_x,
                height: analysis.world.bounds.max_y - analysis.world.bounds.min_y,
              })}
            </p>
          {/if}
          {#if outputs.length}
            <h3>{$t('analyze.outputs')}</h3>
            <ul class="outputs">
              {#each outputs as output (output.name)}
                <li>
                  {recipeName($gameLocale, output.name)}: {formatRate($language, output.net, unit)}
                </li>
              {/each}
            </ul>
          {/if}
          <p>
            {$t('analyze.power_summary', {
              power: formatPower($language, analysis.report.power.average_w),
            })}
          </p>
          {@const warnings = [
            ...analysis.warnings.map((code) => $t(`warning.${code}`)),
            ...analysis.world.warnings.map(worldWarningText),
            ...analysis.report.warnings.map(calcWarningText),
          ]}
          {#if warnings.length}
            <h3>{$t('analyze.warnings')}</h3>
            <ul class="warnings">
              {#each warnings as text, i (i)}
                <li>{text}</li>
              {/each}
              {#if analysis.world.dropped_warnings}
                <li>{$t('warning.dropped', { count: analysis.world.dropped_warnings })}</li>
              {/if}
            </ul>
          {/if}
        {/if}
      {/if}
      {#if !hasIcons}<p class="muted">{$t('analyze.no_icons')}</p>{/if}
    </aside>

    <div class="main">
      <nav class="tabs" aria-label={$t('analyze.tabs')}>
        {#each tabs as name (name)}
          <button
            type="button"
            class="tab"
            class:active={tab === name}
            aria-pressed={tab === name}
            onclick={() => (tab = name)}>{$t(`tab.${name}`)}</button
          >
        {/each}
      </nav>
      <div class="canvas-wrap" class:hidden={tab !== 'drawing'}>
        <div class="canvas" bind:this={canvasHost}></div>
        <div class="canvas-tools">
          <button type="button" class="secondary" onclick={() => scene?.fit()}
            >{$t('analyze.fit')}</button
          >
          {#if analysis?.flow}
            <label class="overlay"
              ><input type="checkbox" bind:checked={overlay} />{$t('analyze.overlay')}</label
            >
          {/if}
          <span class="muted">{$t('analyze.hover_hint')}</span>
        </div>
        {#if hover?.entity}
          <div
            class="tooltip"
            style:left="{hover.screenX + 14}px"
            style:top="{hover.screenY + 14}px"
          >
            <strong>{entityName($gameLocale, hover.entity.name)}</strong>
            {#if hover.entity.recipe}<br />{recipeName($gameLocale, hover.entity.recipe)}{/if}
            <br /><span class="muted">#{hover.entity.entity_number}</span>
          </div>
        {/if}
      </div>
      {#if analysis && tab !== 'drawing'}
        <div class="panel">
          {#if tab === 'production'}
            <ProductionTab report={analysis.report} {unit} />
          {:else if tab === 'flow'}
            <FlowTab
              {analysis}
              {unit}
              {research}
              busy={working}
              onResearch={setResearch}
              {tick}
              {tickState}
              onSimulate={simulateTicks}
              onCancel={cancelTicks}
            />
          {:else if tab === 'sankey'}
            {#if analysis.sankey}
              <SankeyTab sankey={analysis.sankey} {unit} />
            {:else}
              <p class="muted">{$t('flow.unavailable')}</p>
            {/if}
          {:else if tab === 'power'}
            <PowerTab report={analysis.report} {unit} />
          {:else if tab === 'machines'}
            <MachinesTab report={analysis.report} {unit} onRecipe={setRecipe} />
          {:else if tab === 'materials'}
            <MaterialsTab report={analysis.report} />
          {/if}
        </div>
      {:else if tab !== 'drawing'}
        <p class="muted panel">{$t('analyze.nothing_yet')}</p>
      {/if}
    </div>
  </div>
</section>

<style>
  .analyze {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
  }
  .input {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    resize: vertical;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .unit {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .workspace {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 12px;
    flex: 1;
    min-height: 460px;
  }
  .sidebar {
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px;
    overflow-wrap: anywhere;
  }
  .sidebar h3 {
    font-size: 14px;
    margin: 12px 0 4px;
  }
  .tree,
  .tree ul,
  .outputs {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .tree-item {
    background: none;
    border: none;
    color: var(--text);
    text-align: left;
    padding: 2px 4px;
    cursor: pointer;
    width: 100%;
    border-radius: 4px;
    font-weight: 400;
  }
  .tree-item.active {
    background: var(--accent-soft);
  }
  .warnings {
    padding-left: 18px;
    color: var(--warning);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .tab {
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    font-weight: 500;
  }
  .tab.active {
    background: var(--accent);
    color: #111;
  }
  .canvas-wrap {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    flex: 1;
    min-height: 420px;
  }
  .canvas-wrap.hidden {
    display: none;
  }
  .canvas {
    position: absolute;
    inset: 0;
    touch-action: none;
  }
  .canvas-tools {
    position: absolute;
    left: 8px;
    bottom: 8px;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .overlay {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
  }
  .panel {
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px;
    flex: 1;
  }
  .tooltip {
    position: absolute;
    pointer-events: none;
    background: rgba(0, 0, 0, 0.85);
    color: #fff;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 12px;
    max-width: 260px;
  }
  .error {
    color: var(--error);
  }
  .muted {
    color: var(--muted);
  }
  @media (max-width: 760px) {
    .workspace {
      grid-template-columns: 1fr;
    }
  }
</style>
