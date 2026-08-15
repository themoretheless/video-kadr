<script lang="ts">
  import type { CompositionPreviewProxyAdapter } from './compositionPreviewController.js'
  import type { CompositionPreviewSourceRequest } from './compositionPreview.js'

  interface Props {
    source: CompositionPreviewSourceRequest
    adapter: CompositionPreviewProxyAdapter
  }

  let { source, adapter }: Props = $props()
  const selection = $derived(adapter.resolve(source))
  const resolutions = $derived(adapter.resolutions(source))

  $effect(() => {
    adapter.ensure(source)
  })

  function selectResolution(event: Event): void {
    adapter.selectResolution(source, (event.currentTarget as HTMLSelectElement).value)
  }
</script>

{#if selection}
  <section class="composition-proxy-status" aria-label="Источник предпросмотра композиции">
    <span
      class:proxy={selection.kind === 'proxy'}
      class="composition-proxy-indicator"
    >{selection.kind === 'proxy'
      ? `Proxy · ${selection.artifact?.profile.maxWidth}px`
      : selection.fallbackReason ? 'Original · fallback' : 'Original'}</span>

    <label>
      <span>Разрешение предпросмотра</span>
      <select value={selection.artifact?.key ?? 'original'} onchange={selectResolution}>
        {#each resolutions as resolution (resolution.value)}
          <option value={resolution.value}>{resolution.label}</option>
        {/each}
      </select>
    </label>

    <span class="composition-proxy-export-note">Экспорт: Original</span>

    {#if selection.fallbackReason}
      <span class="composition-proxy-fallback" role="status">{selection.fallbackReason}</span>
    {/if}
  </section>
{/if}

<style>
  :global(.composition-proxy-status) {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    min-width: 0;
    font-size: 11px;
  }

  :global(.composition-proxy-indicator) {
    padding: 3px 7px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    color: var(--muted);
    font-weight: 650;
  }

  :global(.composition-proxy-indicator.proxy) {
    border-color: var(--accent);
    color: var(--accent);
  }

  :global(.composition-proxy-status label) {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  :global(.composition-proxy-status select) {
    min-width: 180px;
    padding: 5px 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
  }

  :global(.composition-proxy-export-note) {
    color: var(--faint);
  }

  :global(.composition-proxy-fallback) {
    flex-basis: 100%;
    color: var(--warn);
  }

</style>
