<script lang="ts">
  import { startFilmcraftMaskTrackingJob, readFilmcraftTrackingJob, cancelFilmcraftTrackingJob, importFilmcraftTrackingSnapshot, type FilmcraftTrackingJob, type FilmcraftSourceEditFence, trackFilmcraftMask, type FilmcraftImportResult, type FilmcraftMaskTrackingResult } from '$lib/api'
  let { document, address, sources, timeFrames, timeTicks, token, disabled = false, onbusy, onedit }: { document: string; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string; maskIndex: number; parameter: 'path' }; sources: Record<string, string>; timeFrames: string; timeTicks?: string; token?: string | null; disabled?: boolean; onbusy: (busy: boolean) => void; onedit: (result: FilmcraftImportResult, expected?: FilmcraftSourceEditFence) => boolean | void } = $props()
  let frames = $state('12')
  let busy = $state(false)
  let status = $state('')
  let error = $state('')
  let done = $state(0)
  let total = $state(0)
  let nativeCancel: (() => void) | undefined
  let nativeMode = false
  let controller: AbortController | undefined
  let stop = false
  let alive = true
  const identity = $derived(JSON.stringify([document, address, sources, timeFrames, timeTicks, token, disabled]))
  $effect(() => { void identity; status = ''; error = ''; done = 0; total = 0; return () => { controller?.abort(); nativeCancel?.() } })
  $effect(() => { alive = true; return () => { alive = false; controller?.abort(); nativeCancel?.(); onbusy(false) } })
  async function run(direction: 'forward' | 'backward') {
    if (disabled || busy) return
    const count = Number(frames)
    if (!/^\d+$/.test(frames) || !Number.isInteger(count) || count < 1 || count > 4095) { error = 'Укажите от 1 до 4095 кадров'; return }
    const owner = identity
    const actor = token
    nativeMode = Boolean(actor)
    const fence = { document, sequenceId: address.sequenceId, clipId: address.clipId, token: actor ?? null, sourceBindings: { ...sources }, timeFrames, timeTicks: timeTicks ?? null }
    const current = new AbortController()
    controller = current
    stop = false; busy = true; done = 0; total = count; error = ''; status = 'Трекинг…'; onbusy(true)
    let accepted: FilmcraftImportResult | undefined
    try {
      if (actor) {
        // Keep admission observable after unmount: otherwise an accepted job could lose its ID.
        const started = await startFilmcraftMaskTrackingJob(actor, { document, sequence: address.sequenceId, clip: address.clipId, effect: address.effectIndex, mask: address.maskIndex, frames: count, sources, direction, ...(timeTicks ? { timeTicks } : { timeFrames }) })
        let cancellation: Promise<null> | undefined
        const cancelJob = () => cancellation ??= cancelFilmcraftTrackingJob(actor, started)
        const owned = () => alive && owner === identity && controller === current && !current.signal.aborted
        nativeCancel = () => { void cancelJob().catch(reason => { if (owned()) error = String(reason) }) }
        if (!owned()) { await cancelJob().catch(() => {}); return }
        let job: FilmcraftTrackingJob = started
        while (!job.finished) {
          if (stop) await cancelJob()
          if (!owned()) { await cancelJob().catch(() => {}); return }
          done = job.done; total = job.total; status = `${done} из ${total} кадров`
          await pause(current.signal)
          job = await readFilmcraftTrackingJob(actor, started, false, current.signal)
        }
        if (!owned()) return
        done = job.done; total = job.total
        status = job.result?.error?.startsWith('lost track') ? `Трекинг потерян после ${done} кадров` : job.done < job.total && !job.result?.error ? `Остановлено · сохранено ${done} кадров` : `Сохранено ${done} кадров`
        error = job.result?.error ?? ''
        const snapshot = await readFilmcraftTrackingJob(actor, started, true, current.signal)
        accepted = await importFilmcraftTrackingSnapshot(snapshot, current.signal)
        return
      }
      let previous: FilmcraftMaskTrackingResult | undefined
      for (let step = 0; step < count && !stop; step++) {
        const result = await trackFilmcraftMask({ document: previous?.documentText ?? document, address, sources, direction, ...(previous ? { cursorTicks: previous.tracking.sourceTicks } : {}), ...(timeTicks ? { timeTicks } : { timeFrames }) }, token, current.signal)
        if (!alive || owner !== identity || current.signal.aborted) return
        if (result.tracking.status === 'lost') { status = `Трекинг потерян после ${done} кадров`; break }
        if (result.tracking.status === 'boundary') { status = 'Достигнута граница исходного диапазона'; break }
        previous = result; accepted = result; done++
        status = `${done} кадров · ${result.tracking.inliers} признаков`
        if (!result.tracking.more) break
      }
    } catch (reason) { if (alive && owner === identity && !current.signal.aborted && (actor || !stop)) error = String(reason) }
    finally {
      if (stop && alive && owner === identity && !actor) status = `Остановлено · сохранено ${done} кадров`
      if (alive && owner === identity && accepted && (actor ? !current.signal.aborted : true) && accepted.documentText !== fence.document) { if (onedit(accepted, fence) === false) status = 'Результат не применён: контекст изменился' }
      if (controller === current) { if (actor && !accepted) nativeCancel?.(); busy = false; controller = undefined; nativeCancel = undefined; if (alive) onbusy(false) }
    }
  }
  function cancel() { stop = true; if (nativeMode) nativeCancel?.(); else controller?.abort() }
  function pause(signal: AbortSignal) {
    return new Promise<void>((resolve, reject) => {
      if (signal.aborted) { reject(new DOMException('Aborted', 'AbortError')); return }
      const abort = () => { clearTimeout(timer); reject(new DOMException('Aborted', 'AbortError')) }
      const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve() }, 50)
      signal.addEventListener('abort', abort, { once: true })
    })
  }
</script>
<fieldset disabled={disabled}>
  <legend>Трекинг маски Filmcraft</legend>
  <label>Кадров <input aria-label="Кадров трекинга Filmcraft" bind:value={frames} disabled={busy} /></label>
  <button disabled={busy} onclick={() => run('backward')}>Трекинг маски Filmcraft назад</button>
  <button disabled={busy} onclick={() => run('forward')}>Трекинг маски Filmcraft вперёд</button>
  <button disabled={!busy} onclick={cancel}>Остановить трекинг маски Filmcraft</button>
  {#if busy && total > 0}<progress aria-label="Прогресс трекинга Filmcraft" max={total} value={done}></progress>{/if}
  {#if status}<p role="status">{status}</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
</fieldset>
