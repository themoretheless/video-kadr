<script lang="ts">
  import { untrack } from 'svelte'
  import { nestedDurations } from '$lib/api'
  import { NativeAudioPreview } from '$lib/audio/nativePreview'
  import { nestedComposition, nestedPlayback } from '$lib/state/nestedComposition.svelte'
  let { token, disabled = false }: { token: string; disabled?: boolean } = $props()
  const engine = new NativeAudioPreview()
  const project = $derived(nestedComposition.document)
  const focus = $derived(nestedComposition.activeCompositionId ?? project?.root ?? '')
  let duration = $state(0)
  let durationBusy = $state(false)
  let durationError = $state('')
  let error = $state('')
  let ready = $state(false)
  let drivenTime: number | null = null
  let frame: number | undefined
  $effect(() => {
    const document = project, id = focus
    const controller = new AbortController()
    duration = 0; durationError = ''; durationBusy = !!document
    if (document) void nestedDurations(document, controller.signal).then(values => {
      if (controller.signal.aborted) return
      const value = values[id]
      if (value === undefined || !Number.isSafeInteger(value) || value < 0) throw new Error('Некорректная длительность композиции')
      duration = value
      if (nestedComposition.timeUs > value) nestedComposition.timeUs = value
    }).catch(reason => {
      if (!controller.signal.aborted) {
        durationError = reason instanceof Error ? reason.message : String(reason)
        nestedPlayback.playing = false
      }
    }).finally(() => { if (!controller.signal.aborted) durationBusy = false })
    return () => controller.abort()
  })
  $effect(() => {
    // Navigation/session boundaries always stop playback and clear private buffers.
    void focus; void token
    untrack(() => { nestedPlayback.playing = false; nestedPlayback.epoch++ })
  })
  $effect(() => { if (disabled) nestedPlayback.playing = false })
  $effect(() => {
    const document = project, id = focus, session = token, playing = nestedPlayback.playing, end = duration
    ready = false
    if (document && playing && end > 0) {
      error = ''
      drivenTime = untrack(() => nestedComposition.timeUs)
      untrack(() => { nestedPlayback.epoch++ })
      engine.start(document, drivenTime, end, session, id)
    }
    return () => engine.stop()
  })
  $effect(() => {
    const time = nestedComposition.timeUs
    untrack(() => {
      if (!nestedPlayback.playing || !project || duration <= 0 || time === drivenTime) return
      drivenTime = Math.round(Math.max(0, Math.min(duration, Number.isFinite(time) ? time : 0)))
      if (drivenTime !== time) nestedComposition.timeUs = drivenTime
      nestedPlayback.epoch++
      ready = false; error = ''
      engine.start(project, drivenTime, duration, token, focus)
    })
  })
  function tick(): void {
    frame = undefined
    if (!nestedPlayback.playing) return
    ready = engine.ready
    if (engine.error) { error = engine.error; nestedPlayback.playing = false; return }
    if (duration > 0) {
      drivenTime = engine.finished ? duration : engine.timeUs()
      nestedComposition.timeUs = drivenTime
      if (drivenTime >= duration) { nestedPlayback.playing = false; return }
    }
    frame = requestAnimationFrame(tick)
  }
  $effect(() => {
    if (nestedPlayback.playing && frame === undefined) frame = requestAnimationFrame(tick)
    else if (!nestedPlayback.playing && frame !== undefined) { cancelAnimationFrame(frame); frame = undefined }
  })
  $effect(() => () => {
    if (frame !== undefined) cancelAnimationFrame(frame)
    frame = undefined; engine.stop(); nestedPlayback.playing = false
  })
  function toggle(): void {
    if (!nestedPlayback.playing && nestedComposition.timeUs >= duration) nestedComposition.timeUs = 0
    nestedPlayback.playing = !nestedPlayback.playing
  }
</script>
<div class="composition-transport" aria-label="Воспроизведение вложенной композиции">
  <button class="btn ghost sm" type="button" disabled={disabled || !nestedPlayback.playing && (durationBusy || duration <= 0)} onclick={toggle}>
    {nestedPlayback.playing ? '⏸ Пауза' : '▶ Играть'}
  </button>
  <input aria-label="Плейхед вложенной композиции" type="range" min="0" max={Math.max(1, duration)} step="1000" value={nestedComposition.timeUs} disabled={disabled || durationBusy || duration <= 0} oninput={event => { nestedComposition.timeUs = Number(event.currentTarget.value) }} />
  <span>{(nestedComposition.timeUs / 1e6).toFixed(2)} / {(duration / 1e6).toFixed(2)} с</span>
</div>
{#if durationError || error}<p role="alert">Аудиопредпросмотр: {durationError || error}</p>{/if}
{#if durationBusy || nestedPlayback.playing && !ready}<p role="status">Подготовка нативного звука…</p>{/if}
