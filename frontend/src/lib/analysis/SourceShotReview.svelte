<script lang="ts">
  import { compareLibraryFrames, measureLibraryShots, scanLibraryShots } from '../api.js'
  import { reviewUnplacedShots, type ShotScan } from './shots.js'
  import { measurementKey, shotAnalysisState } from './shotState.svelte.js'
  import ShotCover from './ShotCover.svelte'

  let { sourceId, token, spaceId }: { sourceId: string; token?: string | null; spaceId?: string | null } = $props()
  let scan = $state<ShotScan>()
  let busy = $state(false)
  let error = $state('')
  let threshold = $state(0.3)
  let minimumMs = $state(500)
  let disabledCuts = $state<number[]>([])
  let firstSecond = $state<number | undefined>(0)
  let secondSecond = $state<number | undefined>(0)
  let comparison = $state<Awaited<ReturnType<typeof compareLibraryFrames>>>()
  const lifecycle = new AbortController()
  $effect(() => () => lifecycle.abort())

  const review = $derived.by(() => {
    if (!scan) return { rows: [], error: '' }
    try {
      return { rows: reviewUnplacedShots(scan, threshold, Math.round(minimumMs * 1000), disabledCuts), error: '' }
    } catch (cause) {
      return { rows: [], error: cause instanceof Error ? cause.message : String(cause) }
    }
  })
  const pending = $derived(scan ? review.rows.filter(row => !shotAnalysisState.measurements[measurementKey(scan!, row.startTicks, row.endTicks)]) : [])

  async function analyze() {
    if (busy) return
    busy = true
    error = ''
    try {
      const result = await scanLibraryShots(sourceId, token, spaceId, lifecycle.signal)
      if (lifecycle.signal.aborted) return
      if (scan?.sourceHash !== result.sourceHash) {
        disabledCuts = []
        firstSecond = 0
        secondSecond = Math.floor(result.durationTicks / 2) / 1_000_000
        comparison = undefined
      }
      scan = result
    } catch (cause) {
      if (!lifecycle.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
    } finally { busy = false }
  }

  async function measure() {
    if (busy || !scan || !pending.length) return
    const report = scan, spans = pending.map(({ startTicks, endTicks }) => ({ startTicks, endTicks }))
    busy = true
    error = ''
    try {
      for (let index = 0; index < spans.length; index += 64) {
        const stats = await measureLibraryShots(sourceId, spans.slice(index, index + 64), token, spaceId, report.sourceHash, lifecycle.signal)
        if (lifecycle.signal.aborted) return
        for (const item of stats) shotAnalysisState.measurements[measurementKey(report, item.span.startTicks, item.span.endTicks)] = item
      }
    } catch (cause) {
      if (!lifecycle.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
    } finally { busy = false }
  }

  async function compare() {
    if (busy || !scan || firstSecond === undefined || secondSecond === undefined) return
    const { durationTicks } = scan
    const leftTick = Math.round(firstSecond * 1_000_000)
    const rightTick = Math.round(secondSecond * 1_000_000)
    if (![leftTick, rightTick].every(tick => Number.isSafeInteger(tick) && tick >= 0 && tick < durationTicks)) {
      error = 'Оба кадра должны быть в пределах исходного видео'
      return
    }
    busy = true
    error = ''
    comparison = undefined
    try {
      const result = await compareLibraryFrames(sourceId, leftTick, rightTick, token, spaceId, lifecycle.signal)
      if (!lifecycle.signal.aborted) comparison = result
    } catch (cause) {
      if (!lifecycle.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
    } finally { busy = false }
  }
</script>

<section aria-label="Сцены исходного видео" aria-busy={busy}>
  <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void analyze()}>Анализировать исходник</button>
  {#if scan}
    <label>Порог изменения кадра <input type="range" min={scan.floor} max="1" step="0.01" bind:value={threshold} disabled={busy} /> {threshold.toFixed(2)}</label>
    <label>Минимальная длина сцены, мс <input type="number" min="1" max="60000" bind:value={minimumMs} disabled={busy} /></label>
    <div class="cuts">
      {#each scan.cuts.filter(cut => cut.score > threshold) as cut (cut.tick)}
        <label><input type="checkbox" checked={!disabledCuts.includes(cut.tick)} disabled={busy}
          onchange={event => disabledCuts = event.currentTarget.checked ? disabledCuts.filter(tick => tick !== cut.tick) : [...disabledCuts, cut.tick]} />
          Граница {(cut.tick / 1_000_000).toFixed(3)} с
        </label>
      {/each}
    </div>
    <button class="btn ghost sm" type="button" disabled={busy || !pending.length} onclick={() => void measure()}>Измерить сцены исходника</button>
    <ol aria-label="Сцены исходника">
      {#each review.rows as row, index (row.id)}
        {@const stats = shotAnalysisState.measurements[measurementKey(scan, row.startTicks, row.endTicks)]}
        <li>
          {#key scan.sourceHash}
            <ShotCover {sourceId} {token} {spaceId} sourceTick={row.startTicks + Math.floor((row.endTicks - row.startTicks) / 2)}
              label={`Кадр сцены ${index + 1}`} />
          {/key}
          <span>{(row.startTicks / 1_000_000).toFixed(3)}–{(row.endTicks / 1_000_000).toFixed(3)} с</span>
          {#if stats}<small>Яркость {stats.brightness.toFixed(3)} · движение {stats.motion.toFixed(3)} · резкость {stats.sharpness.toFixed(4)} {stats.flags.join(', ')}</small>{/if}
        </li>
      {/each}
    </ol>
    <form onsubmit={event => { event.preventDefault(); void compare() }}>
      <label>Первый кадр, с
        <input type="number" min="0" max={(scan.durationTicks - 1) / 1_000_000}
          step="any" bind:value={firstSecond} required disabled={busy} />
      </label>
      <label>Второй кадр, с
        <input type="number" min="0" max={(scan.durationTicks - 1) / 1_000_000}
          step="any" bind:value={secondSecond} required disabled={busy} />
      </label>
      <button class="btn ghost sm" type="submit" disabled={busy}>Сравнить кадры исходника</button>
    </form>
    {#if comparison}
      <div class="frame-pair">
        {#each [comparison.leftTick, comparison.rightTick] as tick, index (index)}
          <figure>
            <ShotCover {sourceId} {token} {spaceId} sourceTick={tick} label={index === 0 ? 'Первый кадр' : 'Второй кадр'} />
            <figcaption>{tick / 1_000_000} с</figcaption>
          </figure>
        {/each}
      </div>
      <p role="status">Структурное сходство {comparison.similarity.toFixed(4)} · движение {comparison.motion.toFixed(4)}</p>
    {/if}
  {/if}
  {#if error || review.error}<p role="alert">{error || review.error}</p>{/if}
</section>

<style>
  label { margin-inline-end: 0.5rem; }
  .cuts, ol { max-height: 20rem; overflow: auto; }
  small { display: block; color: var(--muted); }
  li { margin-block: 0.5rem; }
  form, .frame-pair { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
  form input { width: 9ch; }
  figure { margin: 0; width: min(100%, 10rem); }
</style>
