<!--
  PT: Nome e descrição da blueprint. São textos do usuário: só interpolação de texto, nunca HTML.
  EN: Blueprint name and description. They are user text: text interpolation only, never HTML.
-->
<script lang="ts">
  let {
    label,
    description,
    untitled,
  }: { label: string | null; description: string | null; untitled: string } = $props();
</script>

<h2>{label || untitled}</h2>
{#if description}<p class="description">{description}</p>{/if}

<style>
  h2 {
    font-size: 16px;
    margin: 8px 0;
  }
  .description {
    white-space: pre-wrap;
    color: var(--muted);
  }
</style>
