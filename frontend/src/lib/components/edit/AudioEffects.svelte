<script lang="ts">
  import { state as appState } from '$lib/state/store.svelte.js'

  const capability = (id: string) => appState.capabilities?.filters?.find((option) => option.id === id)
  const unavailable = (id: string, fallback: string): string => {
    if (!appState.capabilities) return ''
    const option = capability(id)
    if (!option) return `Нужен обновлённый сервер: ${fallback}`
    return option.available ? '' : option.reason || fallback
  }

  const panUnavailable = $derived(unavailable('audio-pan', 'нужны aformat и stereotools'))
  const eqUnavailable = $derived(unavailable('audio-eq', 'нужен equalizer'))
  const compressorUnavailable = $derived(unavailable('audio-compressor', 'нужен acompressor'))
  const limiterUnavailable = $derived(unavailable('audio-limiter', 'нужен alimiter'))

  function resetAdvancedAudio(): void {
    appState.edit.pan = 0
    appState.edit.audioEqEnabled = false
    appState.edit.audioEq = { lowGainDb: 0, midGainDb: 0, highGainDb: 0 }
    appState.edit.compressorEnabled = false
    appState.edit.compressor = { thresholdDb: -18, ratio: 3, attackMs: 20, releaseMs: 250, makeupGainDb: 0 }
    appState.edit.limiterEnabled = false
    appState.edit.limiter = { ceilingDb: -1, releaseMs: 50 }
  }
</script>

<div class="audio-effects">
  <div class="audio-heading"><strong>Микшер и DSP</strong><button type="button" onclick={resetAdvancedAudio}>Сбросить</button></div>

  <label class:unavailable={Boolean(panUnavailable)} class="audio-slider">
    <span>Панорама <output>{appState.edit.pan < 0 ? `L ${Math.round(-appState.edit.pan * 100)}` : appState.edit.pan > 0 ? `R ${Math.round(appState.edit.pan * 100)}` : 'центр'}</output></span>
    <input type="range" min="-1" max="1" step="0.01" bind:value={appState.edit.pan} disabled={Boolean(panUnavailable)} />
    {#if panUnavailable}<small>{panUnavailable}</small>{/if}
  </label>

  <details class="audio-section">
    <summary><label><input type="checkbox" bind:checked={appState.edit.audioEqEnabled} disabled={Boolean(eqUnavailable)} onclick={(event) => event.stopPropagation()} /> Трёхполосный EQ</label><span aria-hidden="true">⌄</span></summary>
    {#if eqUnavailable}<p class="unavailable-note" role="status">{eqUnavailable}</p>{:else}
      <div class="audio-grid three">
        <label>Низкие, 100 Гц <output>{appState.edit.audioEq.lowGainDb > 0 ? '+' : ''}{appState.edit.audioEq.lowGainDb.toFixed(1)} dB</output><input type="range" min="-24" max="24" step="0.5" bind:value={appState.edit.audioEq.lowGainDb} /></label>
        <label>Средние, 1 кГц <output>{appState.edit.audioEq.midGainDb > 0 ? '+' : ''}{appState.edit.audioEq.midGainDb.toFixed(1)} dB</output><input type="range" min="-24" max="24" step="0.5" bind:value={appState.edit.audioEq.midGainDb} /></label>
        <label>Высокие, 10 кГц <output>{appState.edit.audioEq.highGainDb > 0 ? '+' : ''}{appState.edit.audioEq.highGainDb.toFixed(1)} dB</output><input type="range" min="-24" max="24" step="0.5" bind:value={appState.edit.audioEq.highGainDb} /></label>
      </div>
    {/if}
  </details>

  <details class="audio-section">
    <summary><label><input type="checkbox" bind:checked={appState.edit.compressorEnabled} disabled={Boolean(compressorUnavailable)} onclick={(event) => event.stopPropagation()} /> Компрессор</label><span aria-hidden="true">⌄</span></summary>
    {#if compressorUnavailable}<p class="unavailable-note" role="status">{compressorUnavailable}</p>{:else}
      <div class="audio-grid">
        <label>Порог <output>{appState.edit.compressor.thresholdDb.toFixed(1)} dB</output><input type="range" min="-60" max="0" step="0.5" bind:value={appState.edit.compressor.thresholdDb} /></label>
        <label>Ratio <output>{appState.edit.compressor.ratio.toFixed(1)}:1</output><input type="range" min="1" max="20" step="0.5" bind:value={appState.edit.compressor.ratio} /></label>
        <label>Attack <output>{appState.edit.compressor.attackMs.toFixed(0)} ms</output><input type="range" min="0.1" max="500" step="1" bind:value={appState.edit.compressor.attackMs} /></label>
        <label>Release <output>{appState.edit.compressor.releaseMs.toFixed(0)} ms</output><input type="range" min="1" max="2000" step="5" bind:value={appState.edit.compressor.releaseMs} /></label>
        <label>Makeup <output>{appState.edit.compressor.makeupGainDb > 0 ? '+' : ''}{appState.edit.compressor.makeupGainDb.toFixed(1)} dB</output><input type="range" min="-12" max="24" step="0.5" bind:value={appState.edit.compressor.makeupGainDb} /></label>
      </div>
    {/if}
  </details>

  <details class="audio-section">
    <summary><label><input type="checkbox" bind:checked={appState.edit.limiterEnabled} disabled={Boolean(limiterUnavailable)} onclick={(event) => event.stopPropagation()} /> Лимитер</label><span aria-hidden="true">⌄</span></summary>
    {#if limiterUnavailable}<p class="unavailable-note" role="status">{limiterUnavailable}</p>{:else}
      <div class="audio-grid">
        <label>Ceiling <output>{appState.edit.limiter.ceilingDb.toFixed(1)} dBFS</output><input type="range" min="-24" max="0" step="0.5" bind:value={appState.edit.limiter.ceilingDb} /></label>
        <label>Release <output>{appState.edit.limiter.releaseMs.toFixed(0)} ms</output><input type="range" min="1" max="2000" step="5" bind:value={appState.edit.limiter.releaseMs} /></label>
      </div>
    {/if}
  </details>

  <p class="audio-note">DSP применяется локально в FFmpeg‑экспорте; браузерный preview показывает только громкость.</p>
</div>

<style>
  .audio-effects { display: grid; gap: .6rem; margin-top: .65rem; }
  .audio-heading { display: flex; align-items: center; justify-content: space-between; gap: .7rem; font-size: .78rem; }
  .audio-heading button { padding: .25rem .5rem; border: 1px solid var(--border); border-radius: 6px; background: transparent; color: inherit; cursor: pointer; font-size: .7rem; }
  .audio-section { border: 1px solid var(--border); border-radius: 9px; background: var(--surface-2); overflow: clip; }
  summary { display: flex; justify-content: space-between; gap: .6rem; padding: .65rem .7rem; cursor: pointer; font-size: .76rem; font-weight: 700; }
  summary label { display: flex; align-items: center; gap: .45rem; cursor: pointer; }
  .audio-slider, .audio-grid label { display: grid; gap: .3rem; font-size: .72rem; }
  .audio-slider > span, .audio-grid label { grid-template-columns: 1fr auto; }
  .audio-slider > span { display: grid; }
  .audio-slider input, .audio-grid input { width: 100%; grid-column: 1 / -1; }
  output { color: var(--muted); font-variant-numeric: tabular-nums; }
  .audio-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .7rem; padding: 0 .7rem .75rem; }
  .audio-grid.three { grid-template-columns: 1fr; }
  .audio-grid label { display: grid; }
  .audio-note, .unavailable-note, .audio-slider small { margin: 0; color: var(--muted); font-size: .7rem; }
  .unavailable-note { padding: 0 .7rem .7rem; }
  .audio-slider.unavailable { opacity: .7; }
</style>
