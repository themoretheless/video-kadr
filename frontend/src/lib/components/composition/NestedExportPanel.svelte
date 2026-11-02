<script lang="ts">
  import { cancelJob, exportNestedVideo, getJob, nestedDurations } from '$lib/api'
  import { nestedComposition, nestedHistory } from '$lib/state/nestedComposition.svelte'
  import { loadLibrary } from '$lib/state/store.svelte'
  import type { Job } from '$lib/types'
  let { token, disabled = false }: { token: string; disabled?: boolean } = $props()
  let withAudio = $state(false)
  let startUs = $state(0)
  let endUs = $state(0)
  let duration = $state(0)
  let durationBusy = $state(false)
  let durationError = $state('')
  let busy = $state(false)
  let monitoring = $state(false)
  let cancelling = $state(false)
  let error = $state('')
  let job = $state<Job | null>(null)
  let jobId = $state('')
  const lifecycle = new AbortController()
  let timer: ReturnType<typeof setTimeout> | undefined
  const compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  const validRange = $derived(Number.isSafeInteger(startUs) && Number.isSafeInteger(endUs) && startUs >= 0 && startUs < endUs && endUs <= duration)
  const progress = $derived(Math.min(100, Math.max(0, job?.progress ?? 0)))
  const filename = $derived(job?.status === 'done' && job.result && typeof job.result === 'object' && 'filename' in job.result && typeof job.result.filename === 'string' ? job.result.filename : '')
  $effect(() => {
    const project = nestedComposition.document
    const id = compositionId
    const controller = new AbortController()
    duration = 0; startUs = 0; endUs = 0; durationError = ''
    if (!project || !id) return
    durationBusy = true
    void nestedDurations(project, controller.signal).then((durations) => {
      if (controller.signal.aborted) return
      const value = durations[id]
      if (!Number.isSafeInteger(value) || value === undefined || value < 0) throw new Error('Некорректная длительность композиции')
      duration = value; endUs = value
    }).catch((failure) => {
      if (!controller.signal.aborted) durationError = failure instanceof Error ? failure.message : String(failure)
    }).finally(() => { if (!controller.signal.aborted) durationBusy = false })
    return () => controller.abort()
  })
  $effect(() => () => { lifecycle.abort(); clearTimeout(timer) })
  async function observe(): Promise<void> {
    if (!jobId || lifecycle.signal.aborted) return
    monitoring = true; error = ''
    try {
      const current = await getJob(jobId, lifecycle.signal)
      if (lifecycle.signal.aborted) return
      job = current
      if (current.status === 'done' || current.status === 'cancelled' || current.status === 'error' || current.status === 'interrupted') {
        busy = false; monitoring = false; cancelling = false
        if (current.status === 'error') error = current.error || 'Экспорт завершился с ошибкой'
        if (current.status === 'interrupted') error = 'Задача прервана сервером'
        if (current.status === 'done') void loadLibrary()
        return
      }
      timer = setTimeout(() => void observe(), 1000)
    } catch (failure) {
      if (lifecycle.signal.aborted) return
      monitoring = false
      error = failure instanceof Error ? failure.message : String(failure)
    }
  }
  async function start(): Promise<void> {
    const project = nestedComposition.document
    if (!project || busy || !validRange || disabled || nestedHistory.busy || durationBusy) return
    busy = true; job = null; jobId = ''; error = ''; cancelling = false
    try {
      const hasCutout=Object.values(project.compositions).some(node=>node.content.tracks.some(track=>track.kind==='video'&&track.clips.some(clip=>clip.enabled!==false&&clip.concatCutout)))
      const savedProject=hasCutout&&nestedComposition.projectId&&nestedComposition.revision!==null?{projectId:nestedComposition.projectId,expectedRevision:nestedComposition.revision}:undefined
      const accepted = savedProject?await exportNestedVideo(project, compositionId, startUs, endUs, { ...nestedComposition.renderedOutputs }, token, lifecycle.signal, withAudio ? 'video_audio' : 'video_only',savedProject):await exportNestedVideo(project, compositionId, startUs, endUs, { ...nestedComposition.renderedOutputs }, token, lifecycle.signal, withAudio ? 'video_audio' : 'video_only')
      if (lifecycle.signal.aborted) return
      jobId = accepted.jobId
      await observe()
    } catch (failure) {
      if (!lifecycle.signal.aborted) { busy = false; error = failure instanceof Error ? failure.message : String(failure) }
    }
  }
  async function cancel(): Promise<void> {
    if (!jobId || cancelling) return
    cancelling = true
    await cancelJob(jobId)
    if (!lifecycle.signal.aborted && !monitoring) void observe()
  }
</script>
<section aria-label="Экспорт вложенной композиции" class="nested-export">
  <span>FFV1 / MKV · {withAudio ? 'звук PCM' : 'без звука'}</span>
  <label><input aria-label="Добавить звук в нативный экспорт" type="checkbox" bind:checked={withAudio} disabled={busy || disabled} /> Добавить звук</label>
  <label>От, мкс <input aria-label="Начало нативного экспорта" type="number" min="0" max={duration} step="1" bind:value={startUs} disabled={busy || durationBusy || disabled} /></label>
  <label>До, мкс <input aria-label="Конец нативного экспорта" type="number" min="0" max={duration} step="1" bind:value={endUs} disabled={busy || durationBusy || disabled} /></label>
  <button class="btn primary sm" disabled={busy || durationBusy || disabled || nestedHistory.busy || !validRange} onclick={() => void start()}>Экспортировать видео</button>
  {#if busy && jobId}
    <progress aria-label="Прогресс нативного экспорта" max="100" value={progress}></progress>
    <span>{Math.round(progress)}% {cancelling ? '· отмена запрошена' : ''}</span>
    <button class="btn ghost sm" disabled={cancelling} onclick={() => void cancel()}>Отменить экспорт</button>
    {#if !monitoring}<button class="btn ghost sm" onclick={() => void observe()}>Проверить задачу</button>{/if}
  {/if}
  {#if jobId}<span class="job-id">Задача: {jobId}</span>{/if}
  {#if filename}<span role="status">Готово в медиатеке: {filename}</span>{/if}
  {#if job?.status === 'cancelled'}<span role="status">Экспорт отменён</span>{/if}
  {#if durationError || error}<p role="alert">{durationError || error}</p>{/if}
</section>
<style>
  .nested-export { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-block: 0.5rem; font-size: 0.8rem; }
  label { display: flex; align-items: center; gap: 0.25rem; }
  input[type="number"] { width: 8rem; }
  progress { width: 6rem; }
  .job-id { overflow-wrap: anywhere; }
  p { flex-basis: 100%; margin: 0; }
</style>
