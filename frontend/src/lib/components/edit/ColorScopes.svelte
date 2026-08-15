<script lang="ts">
  import { onMount } from 'svelte'
  import {
    analyzeScopeFrame,
    SCOPE_RESOLUTION,
    WAVEFORM_HEIGHT,
    type ColorHistogram,
    type ColorScopeAnalysis,
  } from '$lib/color/scopes.js'

  interface Props { video?: HTMLVideoElement }
  let { video }: Props = $props()

  type ScopeMode = 'histogram' | 'waveform' | 'vectorscope'
  let mode = $state<ScopeMode>('histogram')
  let canvas: HTMLCanvasElement
  let sourceCanvas: HTMLCanvasElement | null = null
  let analysis = $state.raw<ColorScopeAnalysis | null>(null)
  let error = $state('')
  let opened = $state(false)
  let timer: ReturnType<typeof setInterval> | null = null

  function sample(): void {
    if (!opened || !video || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !canvas) return
    const width = Math.max(1, video.videoWidth)
    const height = Math.max(1, video.videoHeight)
    const scale = Math.min(1, 256 / width, 144 / height)
    const sampleWidth = Math.max(1, Math.round(width * scale))
    const sampleHeight = Math.max(1, Math.round(height * scale))
    sourceCanvas ??= document.createElement('canvas')
    sourceCanvas.width = sampleWidth
    sourceCanvas.height = sampleHeight
    const context = sourceCanvas.getContext('2d', { willReadFrequently: true })
    if (!context) {
      error = 'Браузер не предоставил Canvas 2D для scopes.'
      return
    }
    try {
      context.drawImage(video, 0, 0, sampleWidth, sampleHeight)
      analysis = analyzeScopeFrame(context.getImageData(0, 0, sampleWidth, sampleHeight).data, sampleWidth, sampleHeight)
      error = ''
      renderScope()
    } catch {
      error = 'Кадр недоступен для локального анализа (возможна cross-origin защита).'
    }
  }

  function renderScope(): void {
    if (!canvas || !analysis) return
    if (mode === 'histogram') drawHistogram(canvas, analysis.histogram)
    else if (mode === 'waveform') drawDensity(canvas, analysis.waveform, SCOPE_RESOLUTION, WAVEFORM_HEIGHT, analysis.waveformMaximum, 'waveform')
    else drawDensity(canvas, analysis.vectorscope, SCOPE_RESOLUTION, SCOPE_RESOLUTION, analysis.vectorscopeMaximum, 'vectorscope')
  }

  function selectMode(next: ScopeMode): void {
    mode = next
    queueMicrotask(renderScope)
  }

  $effect(() => {
    const requestedMode = mode
    const requestedAnalysis = analysis
    if (!requestedAnalysis) return
    queueMicrotask(() => {
      if (mode === requestedMode && analysis === requestedAnalysis) renderScope()
    })
  })

  onMount(() => {
    sample()
    timer = setInterval(sample, 250)
    return () => {
      if (timer !== null) clearInterval(timer)
      timer = null
      sourceCanvas = null
    }
  })

  function drawHistogram(target: HTMLCanvasElement, histogram: ColorHistogram): void {
    target.width = SCOPE_RESOLUTION
    target.height = WAVEFORM_HEIGHT
    const context = target.getContext('2d')
    if (!context) return
    context.fillStyle = '#070a10'
    context.fillRect(0, 0, target.width, target.height)
    context.globalCompositeOperation = 'screen'
    for (const [values, color] of [
      [histogram.luma, 'rgba(255,255,255,.45)'],
      [histogram.red, 'rgba(255,70,70,.86)'],
      [histogram.green, 'rgba(60,235,130,.82)'],
      [histogram.blue, 'rgba(70,130,255,.86)'],
    ] as const) {
      context.beginPath()
      context.strokeStyle = color
      for (let x = 0; x < values.length; x += 1) {
        const normalized = histogram.maximum ? Math.log1p(values[x]!) / Math.log1p(histogram.maximum) : 0
        const y = target.height - 1 - normalized * (target.height - 4)
        if (x === 0) context.moveTo(x, y)
        else context.lineTo(x, y)
      }
      context.stroke()
    }
    context.globalCompositeOperation = 'source-over'
  }

  function drawDensity(
    target: HTMLCanvasElement,
    density: Uint32Array,
    width: number,
    height: number,
    maximum: number,
    kind: 'waveform' | 'vectorscope',
  ): void {
    target.width = width
    target.height = height
    const context = target.getContext('2d')
    if (!context) return
    const pixels = context.createImageData(width, height)
    for (let index = 0; index < density.length; index += 1) {
      const strength = maximum ? Math.log1p(density[index]!) / Math.log1p(maximum) : 0
      const offset = index * 4
      pixels.data[offset] = kind === 'vectorscope' ? 70 : 115
      pixels.data[offset + 1] = kind === 'vectorscope' ? 245 : 235
      pixels.data[offset + 2] = kind === 'vectorscope' ? 190 : 255
      pixels.data[offset + 3] = Math.round(strength * 255)
    }
    context.fillStyle = '#070a10'
    context.fillRect(0, 0, width, height)
    context.putImageData(pixels, 0, 0)
    context.strokeStyle = 'rgba(255,255,255,.18)'
    context.lineWidth = 1
    if (kind === 'vectorscope') {
      context.beginPath()
      context.arc(width / 2, height / 2, width * .37, 0, Math.PI * 2)
      context.moveTo(width / 2, 0)
      context.lineTo(width / 2, height)
      context.moveTo(0, height / 2)
      context.lineTo(width, height / 2)
      context.stroke()
    } else {
      for (const y of [0.25, 0.5, 0.75]) {
        context.beginPath()
        context.moveTo(0, Math.round(height * y) + .5)
        context.lineTo(width, Math.round(height * y) + .5)
        context.stroke()
      }
    }
  }
</script>

<details class="color-scopes" ontoggle={(event) => { opened = event.currentTarget.open; if (opened) sample() }}>
  <summary><span><strong>Scopes</strong><small>Локальный анализ текущего исходного кадра</small></span><span aria-hidden="true">⌄</span></summary>
  <div class="scope-toolbar" role="tablist" aria-label="Тип цветового scope">
    <button type="button" role="tab" aria-selected={mode === 'histogram'} class:active={mode === 'histogram'} onclick={() => selectMode('histogram')}>Гистограмма</button>
    <button type="button" role="tab" aria-selected={mode === 'waveform'} class:active={mode === 'waveform'} onclick={() => selectMode('waveform')}>Waveform</button>
    <button type="button" role="tab" aria-selected={mode === 'vectorscope'} class:active={mode === 'vectorscope'} onclick={() => selectMode('vectorscope')}>Vectorscope</button>
  </div>
  <canvas bind:this={canvas} class:vectorscope={mode === 'vectorscope'} aria-label={mode === 'histogram' ? 'Гистограмма текущего кадра' : mode === 'waveform' ? 'Waveform текущего кадра' : 'Vectorscope текущего кадра'}></canvas>
  {#if error}<p class="scope-error" role="status">{error}</p>{:else if !analysis}<p class="scope-hint">Запустите видео или выберите кадр.</p>{:else}<p class="scope-hint">Проанализировано пикселей: {analysis.sampledPixels.toLocaleString('ru-RU')}</p>{/if}
</details>

<style>
  .color-scopes { border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); overflow: clip; }
  summary { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .8rem; cursor: pointer; }
  summary > span:first-child { display: grid; gap: .15rem; }
  summary small, .scope-hint, .scope-error { color: var(--muted); font-size: .75rem; }
  .scope-toolbar { display: grid; grid-template-columns: repeat(3, 1fr); gap: .35rem; padding: 0 .8rem .6rem; }
  .scope-toolbar button { padding: .4rem; border: 1px solid var(--border); border-radius: 7px; background: transparent; color: inherit; cursor: pointer; font-size: .7rem; }
  .scope-toolbar button.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  canvas { display: block; width: calc(100% - 1.6rem); height: 9rem; margin: 0 .8rem; background: #070a10; border-radius: 7px; image-rendering: auto; }
  canvas.vectorscope { height: min(15rem, calc(100vw - 5rem)); object-fit: contain; }
  .scope-hint, .scope-error { margin: 0; padding: .55rem .8rem .8rem; }
  .scope-error { color: var(--danger); }
</style>
