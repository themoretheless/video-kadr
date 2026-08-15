<script lang="ts">
  // Keep the sizeable multitrack/capture/template surface out of the legacy
  // editor's initial route. Vite turns this boundary into a separately cached
  // chunk; switching modes loads it once and preserves normal Svelte state.
  const workspace = import('./CompositionWorkspace.svelte')
</script>

{#await workspace}
  <section class="composition-lazy-status card" role="status" aria-live="polite">
    Загружаю multitrack editor…
  </section>
{:then loaded}
  {@const Workspace = loaded.default}
  <Workspace />
{:catch error}
  <section class="composition-lazy-status card error" role="alert">
    Не удалось загрузить multitrack editor: {error instanceof Error ? error.message : String(error)}
  </section>
{/await}
