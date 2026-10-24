<!--
  PT: Gráfico sankey do regime estável: colunas pela profundidade na cadeia (itens e receitas
      alternados), altura dos nós e largura das ligações pelo fluxo por segundo. O Rust já quebra
      os ciclos (o nó de laço aparece tracejado), então o layout é um grafo sem ciclos.
  EN: Steady-state sankey chart: columns by depth in the chain (items and recipes alternating),
      node height and link width by flow per second. Rust already breaks the cycles (the loop node
      shows dashed), so the layout is an acyclic graph.
-->
<script lang="ts">
  import { recipeName } from '../../lib/game/assets';
  import { gameLocale } from '../../lib/game/store';
  import { formatRate, type RateUnit } from '../../lib/format';
  import { language, t } from '../../lib/i18n';
  import type { Sankey } from '../../lib/types';

  let { sankey, unit }: { sankey: Sankey; unit: RateUnit } = $props();

  const COLUMN = 190;
  const NODE = 12;
  const HEIGHT = 520;
  const GAP = 10;

  interface Box {
    x: number;
    y: number;
    height: number;
    value: number;
    outOffset: number;
    inOffset: number;
  }

  // PT: Profundidade = maior caminho desde uma origem; depois cada coluna é empilhada.
  // EN: Depth = longest path from a source; then each column is stacked.
  const layout = $derived.by(() => {
    const count = sankey.nodes.length;
    const incoming = new Array<number>(count).fill(0);
    const outgoing = new Array<number>(count).fill(0);
    const value = new Array<number>(count).fill(0);
    for (const link of sankey.links) {
      outgoing[link.source] = (outgoing[link.source] ?? 0) + link.per_second;
      incoming[link.target] = (incoming[link.target] ?? 0) + link.per_second;
    }
    for (let i = 0; i < count; i++) value[i] = Math.max(incoming[i] ?? 0, outgoing[i] ?? 0);
    const depth = new Array<number>(count).fill(0);
    for (let round = 0; round < count; round++) {
      let changed = false;
      for (const link of sankey.links) {
        const next = (depth[link.source] ?? 0) + 1;
        if (next > (depth[link.target] ?? 0)) {
          depth[link.target] = next;
          changed = true;
        }
      }
      if (!changed) break;
    }
    const columns: number[][] = [];
    depth.forEach((d, i) => {
      (columns[d] ??= []).push(i);
    });
    // PT: Ordena cada coluna pela altura média dos vizinhos (algumas passadas para frente e para
    //     trás), o que reduz os cruzamentos.
    // EN: Orders each column by the neighbours' mean height (a few forward and backward passes),
    //     which reduces crossings.
    const position = new Array<number>(count).fill(0);
    columns.forEach((nodes) => nodes.forEach((n, i) => (position[n] = i)));
    // PT: Listas de vizinhos montadas uma vez; a chave de cada nó é calculada antes do sort.
    // EN: Neighbour lists built once; each node's key is computed before the sort.
    const before: number[][] = Array.from({ length: count }, () => []);
    const after: number[][] = Array.from({ length: count }, () => []);
    for (const link of sankey.links) {
      before[link.target]?.push(link.source);
      after[link.source]?.push(link.target);
    }
    for (let sweep = 0; sweep < 6; sweep++) {
      const forward = sweep % 2 === 0;
      const order = forward ? columns : [...columns].reverse();
      for (const nodes of order) {
        if (!nodes) continue;
        const keys = new Map<number, number>(
          nodes.map((n) => {
            const near = (forward ? before[n] : after[n]) ?? [];
            const key = near.length
              ? near.reduce((sum, m) => sum + (position[m] ?? 0), 0) / near.length
              : (position[n] ?? 0);
            return [n, key];
          }),
        );
        nodes.sort((a, b) => (keys.get(a) ?? 0) - (keys.get(b) ?? 0));
        nodes.forEach((n, i) => (position[n] = i));
      }
    }
    const maxTotal = Math.max(
      1e-9,
      ...columns.map((nodes) => nodes.reduce((sum, n) => sum + (value[n] ?? 0), 0)),
    );
    const maxNodes = Math.max(1, ...columns.map((nodes) => nodes.length));
    // PT: Altura pelo conteúdo: cada nó tem pelo menos 8 px. EN: Height by content: each node gets at least 8 px.
    const height = Math.max(HEIGHT, maxNodes * (8 + GAP) + GAP);
    const scale = (height - GAP * (maxNodes + 1)) / maxTotal;
    const boxes: Box[] = new Array<Box>(count);
    columns.forEach((nodes, d) => {
      let y = GAP;
      for (const n of nodes) {
        const height = Math.max(2, (value[n] ?? 0) * scale);
        boxes[n] = {
          x: d * COLUMN + 10,
          y,
          height,
          value: value[n] ?? 0,
          outOffset: 0,
          inOffset: 0,
        };
        y += height + GAP;
      }
    });
    const paths = sankey.links.map((link) => {
      const from = boxes[link.source]!;
      const to = boxes[link.target]!;
      const width = Math.max(1, link.per_second * scale);
      const y0 = from.y + from.outOffset + width / 2;
      const y1 = to.y + to.inOffset + width / 2;
      from.outOffset += width;
      to.inOffset += width;
      const x0 = from.x + NODE;
      const x1 = to.x;
      const mid = (x0 + x1) / 2;
      return {
        d: `M${x0},${y0} C${mid},${y0} ${mid},${y1} ${x1},${y1}`,
        width,
        link,
      };
    });
    const columnsCount = Math.max(1, ...depth.map((d) => d + 1));
    return { boxes, paths, width: columnsCount * COLUMN + 140, height };
  });
</script>

{#if sankey.nodes.length === 0}
  <p class="muted">{$t('sankey.empty')}</p>
{:else}
  <div class="scroll">
    <svg width={layout.width} height={layout.height} role="img" aria-label={$t('sankey.title')}>
      {#each layout.paths as path, i (i)}
        <path class="link" d={path.d} stroke-width={path.width}>
          <title
            >{recipeName($gameLocale, sankey.nodes[path.link.source]?.name ?? '')} → {recipeName(
              $gameLocale,
              sankey.nodes[path.link.target]?.name ?? '',
            )}: {formatRate($language, path.link.per_second, unit)}</title
          >
        </path>
      {/each}
      {#each sankey.nodes as node, i (i)}
        {@const box = layout.boxes[i]}
        {#if box}
          <rect
            class:recipe={node.kind === 'recipe'}
            class:loop={node.loop}
            x={box.x}
            y={box.y}
            width={NODE}
            height={box.height}
            rx="2"
          />
          <text
            class:recipe-label={node.kind === 'recipe'}
            x={box.x + NODE + 4}
            y={box.y + box.height / 2}
            dominant-baseline="middle"
          >
            {node.kind === 'recipe' ? '⚙ ' : ''}{recipeName($gameLocale, node.name)}{node.loop
              ? ' ↺'
              : ''}
          </text>
        {/if}
      {/each}
    </svg>
  </div>
  <p class="muted note">{$t('sankey.note')}</p>
{/if}

<style>
  .scroll {
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
  .link {
    fill: none;
    stroke: var(--accent);
    stroke-opacity: 0.28;
  }
  .link:hover {
    stroke-opacity: 0.55;
  }
  rect {
    fill: var(--accent);
  }
  rect.recipe {
    fill: var(--muted);
  }
  rect.loop {
    fill: none;
    stroke: var(--muted);
    stroke-dasharray: 3 2;
  }
  text {
    font-size: 11px;
    fill: var(--text);
  }
  .recipe-label {
    font-style: italic;
    fill: var(--muted);
  }
  .note {
    font-size: 12px;
  }
</style>
