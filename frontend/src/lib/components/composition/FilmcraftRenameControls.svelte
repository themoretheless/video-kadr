<script lang="ts">
  import { renameFilmcraftBin, renameFilmcraftClip, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, name, itemId, binId, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; name: string; itemId?: string; binId?: string; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let draft = $state('')
  let busy = $state(false)
  let error = $state('')
  let status = $state('')
  let controller: AbortController | undefined
  const identity = $derived(JSON.stringify([document, sequenceId, clipId, token, disabled, name, itemId, binId]))
  $effect(() => {
    void identity
    draft = name; error = ''; status = ''
    return () => { controller?.abort() }
  })
  async function rename() {
    if (!token || disabled || busy || draft === name) return
    const expected = { document, sequenceId, clipId, token, ...(itemId ? { itemId } : {}), ...(binId ? { binId } : {}) }
    const captured = identity
    const current = new AbortController()
    controller = current; busy = true; error = ''; status = ''
    try {
      const result = binId ? await renameFilmcraftBin(token, { document, bin: binId, name: draft }, current.signal) : await renameFilmcraftClip(token, { document, name: draft, ...(itemId ? { item: itemId } : { sequence: sequenceId, clip: clipId }) }, current.signal)
      if (current.signal.aborted || identity !== captured) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Имя сохранено'
    } catch (failure) {
      if (!current.signal.aborted && identity === captured) error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      if (controller === current) { controller = undefined; busy = false }
    }
  }
</script>
<form onsubmit={(event) => { event.preventDefault(); void rename() }}>
  <label>{binId ? 'Имя папки' : itemId ? 'Имя элемента проекта' : 'Имя клипа'} <input aria-label={binId ? 'Имя папки Filmcraft' : itemId ? 'Имя элемента проекта Filmcraft' : 'Имя клипа Filmcraft'} bind:value={draft} disabled={disabled || busy || !token} /></label>
  <button type="submit" disabled={disabled || busy || !token || draft === name}>{binId ? 'Переименовать папку' : itemId ? 'Переименовать элемент проекта' : 'Переименовать клип'}</button>
  {#if busy}<span role="status">Сохраняется имя…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</form>
