<script lang="ts">
  import { getAudioProjectResponse, type FilmcraftAudioProjectResponse, type FilmcraftAudioProjectResponseRequest } from '$lib/api'
  import { onDestroy } from 'svelte'
  import { LatestRequest } from '$lib/audio/latestRequest'
  import { cloneDspEffect } from '$lib/composition/dsp'
  import type { CompositionDspEffect } from '$lib/composition/types'
  let { effect: slot, localTick = 0 }: { effect: CompositionDspEffect; localTick?: number } = $props()
  let followCursor = $state(true)
  let open = $state(false)
  let rate = $state(48000)
  let result = $state<FilmcraftAudioProjectResponse | null>(null)
  let error = $state('')
  let loading = $state(false)
  let displayedTime = $state<number | undefined>(undefined)
  let displayedRate = $state(48000)
  const queue = new LatestRequest<FilmcraftAudioProjectResponseRequest, FilmcraftAudioProjectResponse>(
    getAudioProjectResponse, (value, request) => {
      if (request.effect !== slot.id.slice('filmcraft:'.length) || request.sampleRate !== rate || (request.localTimeSeconds !== undefined) !== followCursor) return
      result = value; displayedTime = request.localTimeSeconds; displayedRate = request.sampleRate; error = ''
    }, reason => error = String(reason), value => loading = value,
  )
  onDestroy(() => queue.cancel())
  const levels = Array.from({ length: 49 }, (_, i) => -96 + i * 2)
  let frequencies = $derived(Array.from({ length: 81 }, (_, i) => 20 * (Math.min(20000, rate / 2) / 20) ** (i / 80)))
  $effect(() => {
    if (!open) { queue.cancel(); return }
    const authored = cloneDspEffect(slot)
    queue.push({ effect: authored.id.slice('filmcraft:'.length), params: authored.params, sampleRate: rate, frequencies, levels,
      ...(followCursor ? { animation: authored.animation, localTimeSeconds: localTick / 1000000 } : {}) })
  })
  function path(values: readonly (number | null)[], low: number, high: number): string {
    let connected = false
    return values.map((value, i) => {
      if (value === null || !Number.isFinite(value)) { connected = false; return '' }
      const x = 36 + i / Math.max(1, values.length - 1) * 304
      const y = 12 + (high - Math.max(low, Math.min(high, value))) / (high - low) * 144
      const point = `${connected ? 'L' : 'M'}${x.toFixed(2)},${y.toFixed(2)}`
      connected = true; return point
    }).join(' ')
  }
</script>
<details class="audio-response" ontoggle={event => open = event.currentTarget.open}>
  <summary>Характеристики эффекта</summary>
  <label>Частота дискретизации<select aria-label="Частота дискретизации характеристики" bind:value={rate}>
    <option value={44100}>44,1 кГц</option><option value={48000}>48 кГц</option><option value={96000}>96 кГц</option>
  </select></label>
  <label><input type="checkbox" bind:checked={followCursor} /> Автоматизация на курсоре</label>
  <p class="hint">{followCursor ? `Локальное время клипа: ${(localTick / 1000000).toFixed(3)} с` : 'Для текущих значений полей параметров'}</p>
  {#if loading}<p role="status">Расчёт характеристики…</p>{/if}
  {#if error}<p role="status">{error}</p>{/if}
  {#if result}
    <p class="hint">{displayedRate / 1000} кГц · {displayedTime === undefined ? 'График для значений полей' : `График для времени: ${displayedTime.toFixed(3)} с`}</p>
    {#if result.responseDb.some(value => value !== null)}
      <svg viewBox="0 0 360 180" role="img" aria-label="Частотная характеристика">
        <path class="axes" d="M36,12V156H340 M36,53.14H340" />
        <path class="curve" d={path(result.responseDb, -60, 24)} />
        <text x="2" y="16">24 dB</text><text x="2" y="57">0 dB</text><text x="2" y="157">−60</text>
        <text x="36" y="175">20 Hz</text><text x="290" y="175">{Math.round(frequencies[frequencies.length - 1]! / 1000)} kHz</text>
      </svg>
    {/if}
    {#if result.transferDb.some(value => value !== null)}
      <svg viewBox="0 0 360 180" role="img" aria-label="Характеристика компрессии">
        <path class="axes" d="M36,12V156H340" />
        <path class="curve" d={path(result.transferDb, -96, 24)} />
        <text x="2" y="16">24 dB</text><text x="2" y="157">−96</text>
        <text x="36" y="175">−96 dB in</text><text x="287" y="175">0 dB in</text>
      </svg>
    {/if}
    {#if !result.responseDb.some(value => value !== null) && !result.transferDb.some(value => value !== null)}<p>Эффект не предоставляет статическую характеристику.</p>{/if}
    <p class="hint">Задержка: {result.latencySamples} сэмплов · Прогрев: {result.prerollSamples} сэмплов</p>
  {/if}
</details>
<style>
  .audio-response { margin-block: .35rem; }
  svg { width: 100%; max-width: 24rem; display: block; }
  .axes { stroke: currentColor; opacity: .25; fill: none; }
  .curve { stroke: #65b4ff; stroke-width: 1.5; fill: none; }
  text { fill: currentColor; font-size: 9px; }
  .hint { font-size: .75rem; opacity: .7; margin-block: .25rem; }
</style>
