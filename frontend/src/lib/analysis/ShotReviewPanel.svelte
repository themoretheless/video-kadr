<script lang="ts">
  import { scanLibraryShots, measureLibraryShots, compareLibraryFrames, getLibraryAnalysisFrame } from '../api.js'
  import { compositionState } from '../state/composition/core.svelte.js'
  import { selectedCompositionClip, setCompositionPlayhead } from '../state/composition/transport.svelte.js'
  import { authState } from '../state/auth.svelte.js'
  import { spacesState } from '../state/spaces.svelte.js'
  import { agentSession } from '../agent/session.svelte.js'
  import { reviewShotRows, type ShotRow } from './shots.js'
  import { shotAnalysisState, applyCompositionShotReview, measurementKey, setShotReviewSettings } from './shotState.svelte.js'

  let threshold = $state(compositionState.document.shotReview?.threshold ?? 0.3)
  let minimumMs = $state((compositionState.document.shotReview?.minShotTicks ?? 500_000) / 1000)
  let disabledCuts = $state<number[]>([])
  let discardedIds = $state<string[]>([])
  let busy = $state(false)
  let error = $state('')
  let message = $state('')
  let pair = $state<{ left: string; right: string; similarity: number } | null>(null)
  let alive = true
  let reviewedClipId: string | undefined
  const selected = $derived(selectedCompositionClip())
  const clip = $derived(selected?.kind === 'video' ? selected : null)
  const scan = $derived(clip ? shotAnalysisState.scans[clip.sourceId] : undefined)
  const review = $derived.by(() => {
    if (!clip || !scan) return { rows: [] as ShotRow[], error: '' }
    try { return { rows: reviewShotRows(compositionState.document, clip.id, scan, threshold, Math.round(minimumMs * 1000), disabledCuts), error: '' } }
    catch (cause) { return { rows: [] as ShotRow[], error: cause instanceof Error ? cause.message : String(cause) } }
  })
  const rows = $derived(review.rows)
  const candidates = $derived(clip && scan ? scan.cuts.filter((cut) => cut.tick > clip.sourceInTicks && cut.tick < clip.sourceOutTicks && cut.score > threshold) : [])
  const missingStats = $derived(scan ? rows.filter((row) => !shotAnalysisState.measurements[measurementKey(scan, row.startTicks, row.endTicks)]) : [])

  $effect(() => {
    const id = clip?.id
    if (id === reviewedClipId) return
    reviewedClipId = id
    disabledCuts = []; discardedIds = []; pair = null; error = ''; message = ''
  })
  $effect(() => { const saved = compositionState.document.shotReview; threshold = saved?.threshold ?? 0.3; minimumMs = (saved?.minShotTicks ?? 500_000) / 1000 })
  $effect(() => { const current = pair; return () => { if (current) { URL.revokeObjectURL(current.left); URL.revokeObjectURL(current.right) } } })
  $effect(() => () => { alive = false })

  function settings(): void {
    try { setShotReviewSettings(Number(threshold), Math.round(Number(minimumMs) * 1000)); discardedIds = [] }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
  async function analyze(): Promise<void> {
    if (!clip || busy) return
    const sourceId = clip.sourceId
    busy = true; error = ''
    try {
      const next = await scanLibraryShots(sourceId, authState.token, spacesState.selectedId)
      if (!alive) return
      shotAnalysisState.scans[sourceId] = next
      const keys = Object.keys(shotAnalysisState.scans)
      if (keys.length > 64) delete shotAnalysisState.scans[keys[0]!]
      message = `Найдено кандидатов: ${next.cuts.length}. Порог пересчитывается без повторного сканирования.`
    } catch (cause) { if (alive) error = cause instanceof Error ? cause.message : String(cause) }
    finally { if (alive) busy = false }
  }
  async function measure(): Promise<void> {
    if (!clip || !scan || busy) return
    const sourceId = clip.sourceId
    const report = scan
    const spans = missingStats.map(({ startTicks, endTicks }) => ({ startTicks, endTicks }))
    busy = true; error = ''
    try {
      for (let index = 0; index < spans.length; index += 64) {
        const stats = await measureLibraryShots(sourceId, spans.slice(index, index + 64), authState.token, spacesState.selectedId, report.sourceHash)
        if (!alive) return
        for (const item of stats) shotAnalysisState.measurements[measurementKey(report, item.span.startTicks, item.span.endTicks)] = item
      }
      message = `Измерено сцен: ${spans.length}`
    } catch (cause) { if (alive) error = cause instanceof Error ? cause.message : String(cause) }
    finally { if (alive) busy = false }
  }
  async function compare(row: ShotRow): Promise<void> {
    if (!clip || row.openingCutTick === undefined || busy) return
    const sourceId = clip.sourceId
    const frameTicks = Math.max(1, Math.round(1_000_000 / (compositionState.document.sources[sourceId]?.fps || compositionState.document.canvas.fps)))
    const leftTick = Math.max(0, row.openingCutTick - frameTicks)
    const rightTick = row.openingCutTick
    busy = true; error = ''
    try {
      const comparison = await compareLibraryFrames(sourceId, leftTick, rightTick, authState.token, spacesState.selectedId)
      const [left, right] = await Promise.all([getLibraryAnalysisFrame(sourceId, leftTick, authState.token, spacesState.selectedId), getLibraryAnalysisFrame(sourceId, rightTick, authState.token, spacesState.selectedId)])
      if (!alive || clip?.sourceId !== sourceId) return
      pair = { left: URL.createObjectURL(left), right: URL.createObjectURL(right), similarity: comparison.similarity }
    } catch (cause) { if (alive) error = cause instanceof Error ? cause.message : String(cause) }
    finally { if (alive) busy = false }
  }
  async function apply(action: 'split' | 'mark' | 'discard'): Promise<void> {
    if (!clip || !scan || busy) return
    const clipId = clip.id
    const sourceId = clip.sourceId
    const hash = scan.sourceHash
    const revision = agentSession.revision
    busy = true; error = ''
    try {
      const current = await scanLibraryShots(sourceId, authState.token, spacesState.selectedId)
      if (!alive) return
      if (revision !== agentSession.revision || clip?.id !== clipId) throw new Error('Проект или выбранный клип изменился; повторите просмотр сцен')
      if (current.sourceHash !== hash) {
        shotAnalysisState.scans[sourceId] = current
        disabledCuts = []; discardedIds = []
        throw new Error('Источник изменился; анализ обновлён. Проверьте новые границы')
      }
      applyCompositionShotReview(clipId, threshold, Math.round(minimumMs * 1000), disabledCuts, action, discardedIds.filter((id) => rows.some((row) => row.id === id)))
      message = 'Правка применена одной записью Undo'
    } catch (cause) { if (alive) error = cause instanceof Error ? cause.message : String(cause) }
    finally { if (alive) busy = false }
  }
</script>

<div class="composition-tool-body" aria-busy={busy}>
  {#if !clip}<p>Выберите видеоклип для просмотра сцен.</p>{:else}
    <p>Источник: {clip.sourceId}. Анализ запускается только кнопкой; выбор клипа не запускает сканирование.</p>
    <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void analyze()}>{scan ? 'Проверить сохранённый анализ' : 'Анализировать сцены'}</button>
    {#if scan}
      <label>Порог изменения кадра <input aria-label="Порог изменения кадра" type="range" min={scan.floor} max="1" step="0.01" bind:value={threshold} oninput={settings} disabled={busy} /> {threshold.toFixed(2)}</label>
      <label>Минимальная длина сцены, мс <input aria-label="Минимальная длина сцены, мс" type="number" min="1" max="60000" bind:value={minimumMs} onchange={settings} disabled={busy} /></label>
      <div class="candidate-strip" aria-label="Оценки кандидатов">
        {#each scan.cuts.filter((cut) => cut.tick > clip!.sourceInTicks && cut.tick < clip!.sourceOutTicks) as cut (cut.tick)}
          <span style:left={`${(cut.tick - clip.sourceInTicks) / (clip.sourceOutTicks - clip.sourceInTicks) * 100}%`} style:height={`${cut.score * 100}%`} title={`${(cut.tick / 1_000_000).toFixed(3)}s · ${cut.score.toFixed(3)}`}></span>
        {/each}
        <div class="threshold" style:bottom={`${threshold * 100}%`}></div>
      </div>
      <div class="candidate-checks">
        {#each candidates as cut (cut.tick)}
          <label><input type="checkbox" checked={!disabledCuts.includes(cut.tick)} disabled={busy} onchange={(event) => { disabledCuts = event.currentTarget.checked ? disabledCuts.filter((tick) => tick !== cut.tick) : [...disabledCuts, cut.tick]; discardedIds = [] }} /> Граница {(cut.tick / 1_000_000).toFixed(3)} с · {cut.score.toFixed(3)}</label>
        {/each}
      </div>
      <button class="btn ghost sm" type="button" disabled={busy || !missingStats.length} onclick={() => void measure()}>Измерить сцены</button>
      <ol aria-label="Сцены выбранного клипа">
        {#each rows as row, index (row.id)}
          {@const stats = shotAnalysisState.measurements[measurementKey(scan, row.startTicks, row.endTicks)]}
          <li>
            <label><input type="checkbox" checked={!discardedIds.includes(row.id)} disabled={busy} onchange={(event) => { discardedIds = event.currentTarget.checked ? discardedIds.filter((id) => id !== row.id) : [...discardedIds, row.id] }} /> Оставить сцену {index + 1}</label>
            <button class="btn ghost sm" type="button" onclick={() => setCompositionPlayhead(row.timelineStartTicks)}>{(row.timelineStartTicks / 1_000_000).toFixed(3)}–{(row.timelineEndTicks / 1_000_000).toFixed(3)} с</button>
            {#if row.score !== undefined}<span>Переход: {row.score.toFixed(3)}</span><button class="btn ghost sm" type="button" disabled={busy} onclick={() => void compare(row)}>Сравнить кадры</button>{/if}
            {#if stats}<small>Яркость {stats.brightness.toFixed(3)} · движение {stats.motion.toFixed(3)} · резкость {stats.sharpness.toFixed(4)} · {stats.flags.join(', ')}</small>{:else}<small>Показатели ещё не измерены</small>{/if}
          </li>
        {/each}
      </ol>
      <div>
        <button class="btn ghost sm" type="button" disabled={busy || rows.length < 2} onclick={() => void apply('split')}>Разделить по границам</button>
        <button class="btn ghost sm" type="button" disabled={busy || rows.length < 2} onclick={() => void apply('mark')}>Поставить маркеры</button>
        <button class="btn ghost sm" type="button" disabled={busy || !discardedIds.length} onclick={() => void apply('discard')}>Разделить и исключить снятые сцены</button>
      </div>
    {/if}
  {/if}
  {#if pair}<div class="frame-pair"><img src={pair.left} alt="Кадр перед границей" /><img src={pair.right} alt="Кадр после границы" /></div><p>Структурное сходство: {pair.similarity.toFixed(4)}</p>{/if}
  {#if error || review.error}<p class="error" role="alert">{error || review.error}</p>{/if}
  {#if message}<p role="status">{message}</p>{/if}
</div>

<style>
  .candidate-strip { position: relative; height: 64px; background: var(--panel-2); overflow: hidden; }
  .candidate-strip span { position: absolute; bottom: 0; width: 2px; background: var(--accent); }
  .threshold { position: absolute; left: 0; right: 0; border-top: 1px solid var(--text); }
  .candidate-checks { display: flex; flex-wrap: wrap; gap: 8px; max-height: 100px; overflow: auto; }
  ol { max-height: 380px; overflow: auto; padding-left: 24px; }
  li { margin: 8px 0; }
  small { display: block; color: var(--muted); }
  .frame-pair { display: flex; gap: 8px; }
  .frame-pair img { width: calc(50% - 4px); object-fit: contain; }
</style>
