<script lang="ts">
  import { tick } from 'svelte'
  import {
    COMPOSITION_TIME_BASE,
    clipEndTicks,
    type Composition,
    type CompositionVisualAnimation,
    type VideoClip,
    type VisualClip,
  } from '$lib/composition/types.js'
  import { primaryCompositionVideoTrack } from '$lib/composition/validation.js'
  import {
    sampleBrowserVideoLumaFrames,
    type BrowserTrackingFrames,
  } from '$lib/motion/browserFrameSampler.js'
  import {
    buildCompositionPointTrackingPlan,
    pointTrackToTargetAnimation,
    type CompositionPointTrackingPlan,
  } from '$lib/motion/compositionTracking.js'
  import { trackPointSequence, type GrayFrame, type TrackPoint } from '$lib/motion/tracker.js'

  interface Props {
    document: Composition
    media: Readonly<Record<string, { readonly url: string }>>
    onapply: (clipId: string, animation: CompositionVisualAnimation) => void | Promise<void>
  }

  let { document, media, onapply }: Props = $props()
  let sourceClipId = $state('')
  let targetClipId = $state('')
  let rangeStartSeconds = $state(0)
  let rangeEndSeconds = $state(0)
  let sampleFps = $state(10)
  let patchRadius = $state(4)
  let searchRadius = $state(12)
  let minimumConfidence = $state(0.55)
  let prepared = $state<BrowserTrackingFrames | null>(null)
  let preparedPlan = $state<CompositionPointTrackingPlan | null>(null)
  let selectedPoint = $state<TrackPoint | null>(null)
  let busy = $state(false)
  let message = $state('')
  let error = $state('')
  let selectionKey = $state('')
  let abortController = $state<AbortController | null>(null)
  let previewCanvas = $state<HTMLCanvasElement>()

  const primaryTrack = $derived(primaryCompositionVideoTrack(document))
  const sourceClips = $derived((primaryTrack?.clips ?? []).filter(trackableCandidate))
  const targetClips = $derived.by(() => document.tracks.flatMap((track) => {
    if (track.kind === 'audio' || track.id === primaryTrack?.id || track.locked) return []
    return track.clips as readonly VisualClip[]
  }))

  $effect(() => {
    const sources = sourceClips
    const targets = targetClips
    if (!sources.some((clip) => clip.id === sourceClipId)) sourceClipId = sources[0]?.id ?? ''
    if (!targets.some((clip) => clip.id === targetClipId)) targetClipId = targets[0]?.id ?? ''
    const nextKey = `${sourceClipId}:${targetClipId}`
    if (nextKey === selectionKey) return
    selectionKey = nextKey
    const overlap = selectedOverlap()
    rangeStartSeconds = overlap ? overlap.start / COMPOSITION_TIME_BASE : 0
    rangeEndSeconds = overlap ? Math.min(overlap.end, overlap.start + 10 * COMPOSITION_TIME_BASE) / COMPOSITION_TIME_BASE : 0
    resetPrepared()
  })

  $effect(() => () => {
    abortController?.abort()
    abortController = null
    if (previewCanvas) {
      previewCanvas.width = 0
      previewCanvas.height = 0
    }
  })

  function trackableCandidate(clip: VideoClip): boolean {
    return (clip.playbackMode?.mode ?? 'forward') === 'forward' &&
      (clip.stabilization?.mode ?? 'disabled') === 'disabled' &&
      !(primaryTrack?.transitions ?? []).some(
        (transition) => transition.fromClipId === clip.id || transition.toClipId === clip.id,
      )
  }

  function selectedSource(): VideoClip | undefined {
    return sourceClips.find((clip) => clip.id === sourceClipId)
  }

  function selectedTarget(): VisualClip | undefined {
    return targetClips.find((clip) => clip.id === targetClipId)
  }

  function selectedOverlap(): { start: number; end: number } | null {
    const source = selectedSource()
    const target = selectedTarget()
    if (!source || !target) return null
    const start = Math.max(source.timelineStartTicks, target.timelineStartTicks)
    const end = Math.min(clipEndTicks(source), clipEndTicks(target))
    return end > start ? { start, end } : null
  }

  function resetPrepared(): void {
    abortController?.abort()
    abortController = null
    prepared = null
    preparedPlan = null
    selectedPoint = null
    message = ''
    error = ''
  }

  function cancelPreparation(): void {
    abortController?.abort()
  }

  async function prepareFrames(): Promise<void> {
    if (busy) return
    const sourceClip = selectedSource()
    if (!sourceClip) return
    const sourceMedia = media[sourceClip.sourceId]
    if (!sourceMedia?.url) {
      error = 'Tracking source отсутствует локально; сначала выполните relink.'
      return
    }
    busy = true
    error = ''
    message = ''
    prepared = null
    selectedPoint = null
    abortController?.abort()
    const controller = new AbortController()
    abortController = controller
    try {
      const plan = buildCompositionPointTrackingPlan(document, sourceClipId, targetClipId, {
        startTicks: Math.round(rangeStartSeconds * COMPOSITION_TIME_BASE),
        endTicks: Math.round(rangeEndSeconds * COMPOSITION_TIME_BASE),
        sampleFps,
      })
      const frames = await sampleBrowserVideoLumaFrames(sourceMedia.url, plan.sampleSourceSeconds, {
        signal: controller.signal,
      })
      if (controller.signal.aborted) return
      preparedPlan = plan
      prepared = frames
      await tick()
      drawGrayFrame(frames.frames[0]!)
      message = `Подготовлено кадров: ${frames.frames.length}. Выберите контрастную точку на первом кадре.`
    } catch (caught) {
      if (!(caught instanceof DOMException && caught.name === 'AbortError')) {
        error = caught instanceof Error ? caught.message : String(caught)
      }
    } finally {
      if (abortController === controller) abortController = null
      busy = false
    }
  }

  function choosePoint(event: MouseEvent): void {
    if (!prepared || !previewCanvas) return
    const rect = previewCanvas.getBoundingClientRect()
    const x = event.detail === 0
      ? Math.floor(prepared.width / 2)
      : Math.floor(((event.clientX - rect.left) / Math.max(1, rect.width)) * prepared.width)
    const y = event.detail === 0
      ? Math.floor(prepared.height / 2)
      : Math.floor(((event.clientY - rect.top) / Math.max(1, rect.height)) * prepared.height)
    selectedPoint = {
      x: Math.max(patchRadius, Math.min(prepared.width - patchRadius - 1, x)),
      y: Math.max(patchRadius, Math.min(prepared.height - patchRadius - 1, y)),
    }
    drawGrayFrame(prepared.frames[0]!, selectedPoint)
    message = `Точка: ${selectedPoint.x}, ${selectedPoint.y}. Запустите tracking.`
  }

  async function applyTracking(): Promise<void> {
    if (busy || !prepared || !preparedPlan || !selectedPoint) return
    const target = selectedTarget()
    if (!target) return
    busy = true
    error = ''
    message = ''
    try {
      const result = trackPointSequence(prepared.frames, selectedPoint, {
        patchRadius,
        searchRadius,
        minimumConfidence,
      })
      const animation = pointTrackToTargetAnimation(
        preparedPlan,
        result,
        target.animation,
        1.5,
        { x: prepared.sampleToSourceScaleX, y: prepared.sampleToSourceScaleY },
      )
      await onapply(target.id, animation)
      const minimum = Math.min(...result.points.map((point) => point.confidence))
      message = `Tracking применён: ${result.points.length} samples, minimum confidence ${minimum.toFixed(2)}.`
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught)
    } finally {
      busy = false
    }
  }

  function drawGrayFrame(frame: GrayFrame, point?: TrackPoint): void {
    const canvas = previewCanvas
    if (!canvas) return
    canvas.width = frame.width
    canvas.height = frame.height
    const context = canvas.getContext('2d')
    if (!context) return
    const rgba = new Uint8ClampedArray(frame.data.length * 4)
    for (let index = 0; index < frame.data.length; index += 1) {
      const value = frame.data[index]!
      const offset = index * 4
      rgba[offset] = value
      rgba[offset + 1] = value
      rgba[offset + 2] = value
      rgba[offset + 3] = 255
    }
    context.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0)
    if (point) {
      context.strokeStyle = '#ff375f'
      context.lineWidth = Math.max(1, frame.width / 320)
      context.strokeRect(
        point.x - patchRadius,
        point.y - patchRadius,
        patchRadius * 2 + 1,
        patchRadius * 2 + 1,
      )
    }
  }
</script>

<details class="composition-tool-section composition-tracker" aria-label="Classical point tracking">
  <summary>Classical point tracking</summary>
  <div class="composition-tool-body" aria-busy={busy}>
    <p>Локальный ZNCC tracker без моделей. Он читает до 300 уменьшенных кадров и записывает обычные X/Y keyframes overlay.</p>
    {#if !sourceClips.length || !targetClips.length}
      <p>Нужны primary forward video без transition/deshake и отдельный незаблокированный visual overlay.</p>
    {:else}
      <div class="tracker-grid">
        <label>
          Tracking source
          <select bind:value={sourceClipId} onchange={resetPrepared} disabled={busy}>
            {#each sourceClips as clip (clip.id)}<option value={clip.id}>{clip.id}</option>{/each}
          </select>
        </label>
        <label>
          Target overlay
          <select bind:value={targetClipId} onchange={resetPrepared} disabled={busy}>
            {#each targetClips as clip (clip.id)}<option value={clip.id}>{clip.id} · {clip.kind}</option>{/each}
          </select>
        </label>
        <label>Начало, с<input aria-label="Начало tracking range" type="number" min="0" step="0.001" bind:value={rangeStartSeconds} onchange={resetPrepared} disabled={busy} /></label>
        <label>Конец, с<input aria-label="Конец tracking range" type="number" min="0" step="0.001" bind:value={rangeEndSeconds} onchange={resetPrepared} disabled={busy} /></label>
        <label>Sample FPS<select aria-label="Tracking sample FPS" bind:value={sampleFps} onchange={resetPrepared} disabled={busy}><option value={5}>5</option><option value={10}>10</option><option value={15}>15</option><option value={30}>30</option></select></label>
        <label>Patch radius<input aria-label="Tracking patch radius" type="number" min="2" max="32" step="1" bind:value={patchRadius} onchange={resetPrepared} disabled={busy} /></label>
        <label>Search radius<input aria-label="Tracking search radius" type="number" min="2" max="64" step="1" bind:value={searchRadius} onchange={resetPrepared} disabled={busy} /></label>
        <label>Confidence<input aria-label="Minimum tracking confidence" type="number" min="0" max="1" step="0.05" bind:value={minimumConfidence} disabled={busy} /></label>
      </div>
      <div class="composition-tool-actions">
        <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void prepareFrames()}>{busy && !prepared ? 'Декодирую…' : 'Подготовить кадры'}</button>
        {#if busy && abortController}<button class="btn ghost sm" type="button" aria-label="Отменить подготовку tracking кадров" onclick={cancelPreparation}>Отмена</button>{/if}
      </div>
      {#if prepared}
        <button class="tracker-frame" type="button" aria-label="Выбрать tracking point на первом кадре" onclick={choosePoint} disabled={busy}>
          <canvas bind:this={previewCanvas} aria-hidden="true"></canvas>
        </button>
        <button class="btn primary sm" type="button" disabled={busy || !selectedPoint} onclick={() => void applyTracking()}>Отследить и применить X/Y</button>
      {/if}
    {/if}
    {#if message}<p role="status">{message}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  .tracker-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.55rem;
  }

  .tracker-grid label {
    min-width: 0;
  }

  .tracker-grid input,
  .tracker-grid select {
    width: 100%;
    min-width: 0;
  }

  .tracker-frame {
    display: block;
    width: min(100%, 40rem);
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    background: #000;
    cursor: crosshair;
  }

  .tracker-frame canvas {
    display: block;
    width: 100%;
    height: auto;
  }
</style>
