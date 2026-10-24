<!--
  PT: Ícone recortado do atlas com CSS (posição de fundo). Sem atlas, mostra só um quadrado neutro.
      O nome vem dos dados do jogo e é usado só como chave do atlas e como texto alternativo.
  EN: Icon cut from the atlas with CSS (background position). Without an atlas, it shows a
      neutral square. The name comes from the game data and is only used as an atlas key and as
      alternative text.
-->
<script lang="ts">
  import { atlasState } from '../game/store';

  let {
    kind,
    name,
    size = 20,
    title = name,
  }: {
    kind: 'item' | 'fluid' | 'recipe' | 'entity' | 'virtual-signal';
    name: string;
    size?: number;
    title?: string;
  } = $props();

  const keys = $derived(
    kind === 'item'
      ? [`item/${name}`, `fluid/${name}`]
      : kind === 'recipe'
        ? [`recipe/${name}`, `item/${name}`, `fluid/${name}`]
        : [`${kind}/${name}`, `item/${name}`],
  );
  const slot = $derived.by(() => {
    const state = $atlasState;
    if (!state) return null;
    for (const key of keys) {
      const found = state.atlas.icons[key];
      const page = found ? state.atlas.pages[found.page] : undefined;
      if (found && page) return { found, url: state.pageUrl(page), cell: state.atlas.cell };
    }
    return null;
  });
</script>

{#if slot}
  <span
    class="icon"
    role="img"
    aria-label={title}
    {title}
    style:width="{size}px"
    style:height="{size}px"
    style:background-image="url({slot.url})"
    style:background-size="{(2048 / slot.cell) * size}px {(2048 / slot.cell) * size}px"
    style:background-position="-{slot.found.col * size}px -{slot.found.row * size}px"
  ></span>
{:else}
  <span
    class="icon empty"
    role="img"
    aria-label={title}
    {title}
    style:width="{size}px"
    style:height="{size}px"
  ></span>
{/if}

<style>
  .icon {
    display: inline-block;
    vertical-align: middle;
    background-repeat: no-repeat;
    flex: none;
  }
  .empty {
    background: var(--border);
    border-radius: 3px;
  }
</style>
