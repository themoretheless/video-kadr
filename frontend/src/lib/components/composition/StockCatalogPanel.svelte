<script lang="ts">
  import { searchStockCatalog, type StockAssetDto, type StockKind } from '$lib/api.js'
  import { authState } from '$lib/state/auth.svelte.js'
  import { addMediaInfoToComposition, editorMode } from '$lib/state/composition.svelte.js'
  import { doImport, doUpload, state as legacyState } from '$lib/state/store.svelte.js'

  const MAX_STOCK_PHOTO_BYTES = 64 * 1024 * 1024
  let query = $state('')
  let kind = $state<StockKind>('video')
  let orientation = $state<'' | 'landscape' | 'portrait' | 'square'>('')
  let result = $state<Awaited<ReturnType<typeof searchStockCatalog>> | null>(null)
  let busy = $state(false)
  let importing = $state<number | null>(null)
  let error = $state('')

  async function search(page = 1): Promise<void> {
    const token = authState.token
    if (!token || !query.trim() || busy) return
    busy = true; error = ''
    try { result = await searchStockCatalog(token, query.trim(), kind, orientation, page) }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function importAsset(asset: StockAssetDto): Promise<void> {
    if (importing !== null || legacyState.importing) return
    importing = asset.providerId; error = ''
    try {
      let media
      if (asset.mediaType === 'video') {
        legacyState.url = asset.importUrl
        legacyState.importStart = ''
        legacyState.importEnd = ''
        media = await doImport()
      } else {
        const response = await fetch(asset.importUrl)
        if (!response.ok) throw new Error(`Pexels image download: HTTP ${response.status}`)
        const declared = Number(response.headers.get('content-length') ?? 0)
        if (declared > MAX_STOCK_PHOTO_BYTES) throw new Error('Stock image превышает лимит 64 МиБ')
        const blob = await readBoundedBlob(response)
        media = await doUpload(new File([blob], `pexels-${asset.providerId}.jpg`, { type: blob.type || 'image/jpeg' }), false)
      }
      if (media && editorMode.value === 'composition') addMediaInfoToComposition(media)
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { importing = null }
  }

  async function readBoundedBlob(response: Response): Promise<Blob> {
    const reader = response.body?.getReader()
    if (!reader) throw new Error('Браузер не поддерживает потоковый импорт stock image')
    const chunks: ArrayBuffer[] = []
    let total = 0
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      total += value.byteLength
      if (total > MAX_STOCK_PHOTO_BYTES) {
        await reader.cancel()
        throw new Error('Stock image превышает лимит 64 МиБ')
      }
      chunks.push(value.slice().buffer as ArrayBuffer)
    }
    return new Blob(chunks, { type: response.headers.get('content-type') || 'image/jpeg' })
  }
</script>

<details class="composition-tool-section stock-catalog">
  <summary>Стоковые фото и видео</summary>
  <div class="composition-tool-body">
    <p>Licensed media для монтажа. Поиск предоставлен <a href="https://www.pexels.com" target="_blank" rel="noreferrer">Pexels</a>; автор указан у каждого результата.</p>
    {#if authState.token}
      <form onsubmit={(event) => { event.preventDefault(); void search() }}>
        <input aria-label="Поиск Pexels" maxlength="100" bind:value={query} placeholder="city night, nature…" />
        <select aria-label="Тип stock media" bind:value={kind}><option value="video">Видео</option><option value="photo">Фото</option></select>
        <select aria-label="Ориентация stock media" bind:value={orientation}><option value="">Любая</option><option value="landscape">Landscape</option><option value="portrait">Portrait</option><option value="square">Square</option></select>
        <button class="btn primary sm" disabled={busy || !query.trim()}>{busy ? 'Поиск…' : 'Найти'}</button>
      </form>
      {#if result}
        <p>{result.totalResults.toLocaleString()} результатов · страница {result.page}</p>
        <ul class="stock-grid">
          {#each result.assets as asset (asset.providerId)}
            <li><img src={asset.previewUrl} alt={asset.title} loading="lazy" referrerpolicy="no-referrer" /><strong>{asset.title}</strong><a href={asset.sourcePageUrl} target="_blank" rel="noreferrer">{asset.author} · Pexels</a><small>{asset.width}×{asset.height}{asset.duration ? ` · ${asset.duration} с` : ''}</small><button class="btn ghost sm" disabled={importing !== null} onclick={() => void importAsset(asset)}>{importing === asset.providerId ? 'Импорт…' : 'Добавить в проект'}</button></li>
          {/each}
        </ul>
        <div class="composition-tool-actions"><button class="btn ghost sm" disabled={busy || result.page <= 1} onclick={() => void search(result!.page - 1)}>Назад</button><button class="btn ghost sm" disabled={busy || result.page * 24 >= result.totalResults} onclick={() => void search(result!.page + 1)}>Дальше</button></div>
      {/if}
    {:else}<p>Войдите, чтобы использовать лимит провайдера безопасно через backend.</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  form { display: grid; grid-template-columns: minmax(160px, 1fr) auto auto auto; gap: 7px; }
  .stock-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; margin: 0; padding: 0; list-style: none; }
  .stock-grid li { display: grid; gap: 5px; padding: 8px; border: 1px solid var(--border); border-radius: 9px; }
  .stock-grid img { width: 100%; aspect-ratio: 16 / 9; object-fit: cover; border-radius: 6px; background: #111; }
  .stock-grid a, .stock-grid small { font-size: .72rem; color: var(--muted); }
  .error { color: var(--danger); }
  @media (max-width: 700px) { form { grid-template-columns: 1fr; } }
</style>
