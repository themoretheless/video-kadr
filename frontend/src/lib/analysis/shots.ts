import { findClipLocation, splitClip, deleteClip } from '../composition/commands.js'
import { clipEndTicks, type Composition, type VideoClip } from '../composition/types.js'
import { assertValidComposition } from '../composition/validation.js'
import { speedRampTimelineTickAtSourceProgress } from '../composition/speedRamp.js'
import { upsertCompositionMarker } from '../composition/markers.js'

export interface ShotCut { tick: number; score: number }
export interface ShotScan { version: 1; sourceHash: string; durationTicks: number; floor: number; cuts: ShotCut[] }
export interface ShotSpan { startTicks: number; endTicks: number }
export interface ShotMeasurement { span: ShotSpan; brightness: number; motion: number; sharpness: number; flags: string[] }
export interface ShotRow extends ShotSpan { id: string; timelineStartTicks: number; timelineEndTicks: number; openingCutTick?: number; score?: number }

export function validateShotScan(value: unknown): asserts value is ShotScan {
  const scan = value as ShotScan
  if (!scan || scan.version !== 1 || !/^[a-f0-9]{64}$/.test(scan.sourceHash) || scan.floor !== 0.05 || !Number.isSafeInteger(scan.durationTicks) || scan.durationTicks <= 0 || scan.durationTicks > 86_400_000_000 || !Array.isArray(scan.cuts) || scan.cuts.length > 10_000) throw new Error('Invalid source shot scan')
  let previous = 0
  for (const cut of scan.cuts) {
    if (!Number.isSafeInteger(cut.tick) || cut.tick <= previous || cut.tick >= scan.durationTicks || !Number.isFinite(cut.score) || cut.score <= scan.floor || cut.score > 1) throw new Error('Invalid shot candidate')
    previous = cut.tick
  }
}

/** Threshold changes are a pure reduction over the one source-absolute floor scan. */
export function reviewShotRows(document: Composition, clipId: string, scan: ShotScan, threshold = 0.3, minShotTicks = 500_000, disabledCuts: readonly number[] = []): ShotRow[] {
  validateShotScan(scan)
  if (!Number.isFinite(threshold) || threshold < scan.floor || threshold > 1 || !Number.isSafeInteger(minShotTicks) || minShotTicks < 1 || minShotTicks > 60_000_000) throw new Error('Invalid shot review settings')
  const { clip } = findClipLocation(document, clipId)
  if (clip.kind !== 'video') throw new Error('Shot review requires a video clip')
  if (clip.playbackMode?.mode === 'freeze') return [{ id: `${clip.sourceInTicks}-${clip.sourceOutTicks}`, startTicks: clip.sourceInTicks, endTicks: clip.sourceOutTicks, timelineStartTicks: clip.timelineStartTicks, timelineEndTicks: clipEndTicks(clip) }]
  if (clip.sourceOutTicks > scan.durationTicks) throw new Error('Shot scan does not cover source window')
  const boundaries = [clip.sourceInTicks]
  for (const cut of scan.cuts) {
    if (cut.score > threshold && cut.tick > clip.sourceInTicks && cut.tick < clip.sourceOutTicks && !disabledCuts.includes(cut.tick) && cut.tick - boundaries.at(-1)! >= minShotTicks) boundaries.push(cut.tick)
  }
  if (boundaries.length > 1 && clip.sourceOutTicks - boundaries.at(-1)! < minShotTicks) boundaries.pop()
  boundaries.push(clip.sourceOutTicks)
  const rows = boundaries.slice(0, -1).map((startTicks, index): ShotRow => {
    const endTicks = boundaries[index + 1]!
    const reverse = clip.playbackMode?.mode === 'reverse'
    const timelineStartTicks = sourceBoundaryToTimeline(clip, reverse ? endTicks : startTicks)
    const timelineEndTicks = sourceBoundaryToTimeline(clip, reverse ? startTicks : endTicks)
    const opening = reverse ? endTicks : startTicks
    const cut = scan.cuts.find((candidate) => candidate.tick === opening)
    return { id: `${startTicks}-${endTicks}`, startTicks, endTicks, timelineStartTicks, timelineEndTicks, ...(cut && timelineStartTicks > clip.timelineStartTicks ? { openingCutTick: opening, score: cut.score } : {}) }
  })
  return rows.sort((a, b) => a.timelineStartTicks - b.timelineStartTicks)
}

export function sourceBoundaryToTimeline(clip: VideoClip, sourceTick: number): number {
  const progress = clip.playbackMode?.mode === 'reverse' ? clip.sourceOutTicks - sourceTick : sourceTick - clip.sourceInTicks
  const local = clip.speedRamp ? speedRampTimelineTickAtSourceProgress(clip.sourceOutTicks - clip.sourceInTicks, clip.speed ?? 1, clip.speedRamp, progress) : Math.round(progress / (clip.speed ?? 1))
  return clip.timelineStartTicks + local
}

/** All three review verbs consume the same frame-snapped boundary list. */
export function applyShotReview(document: Composition, clipId: string, rows: readonly ShotRow[], action: 'split' | 'mark' | 'discard', discardedRowIds: readonly string[], idForSegment: (index: number) => string): { document: Composition; selectedClipId: string } {
  assertValidComposition(document)
  const location = findClipLocation(document, clipId)
  if (location.track.locked) throw new Error('Дорожка заблокирована')
  if (location.clip.kind !== 'video' || location.clip.playbackMode?.mode === 'freeze') throw new Error('Разбиение сцен требует обычный video clip')
  if (!rows.length || rows.length > 256) throw new Error('Review requires 1–256 shots')
  if (rows[0]!.timelineStartTicks !== location.clip.timelineStartTicks || rows.at(-1)!.timelineEndTicks !== clipEndTicks(location.clip) || rows.some((row, index) => row.timelineEndTicks <= row.timelineStartTicks || (index > 0 && row.timelineStartTicks !== rows[index - 1]!.timelineEndTicks))) throw new Error('Shot rows do not cover the selected clip')
  if (discardedRowIds.some((id) => !rows.some((row) => row.id === id))) throw new Error('Unknown discarded shot')
  if (action === 'discard' && rows.every((row) => discardedRowIds.includes(row.id))) throw new Error('Нельзя исключить все сцены; используйте удаление клипа')
  const boundaries = rows.slice(1).map((row) => Math.round(Math.round(row.timelineStartTicks * document.canvas.fps / 1_000_000) * 1_000_000 / document.canvas.fps))
  if (boundaries.some((tick, index) => tick <= location.clip.timelineStartTicks || tick >= clipEndTicks(location.clip) || (index > 0 && tick <= boundaries[index - 1]!))) throw new Error('Границы совпали на сетке кадров; увеличьте минимальную длину сцены')
  let result = document
  if (action === 'mark') {
    for (let index = 0; index < boundaries.length; index++) result = upsertCompositionMarker(result, { id: idForSegment(index), tick: boundaries[index]!, label: `Shot ${index + 2}` })
    return { document: result, selectedClipId: clipId }
  }
  const segmentIds = [clipId]
  for (let index = 0; index < boundaries.length; index++) {
    const rightId = idForSegment(index)
    result = splitClip(result, segmentIds.at(-1)!, boundaries[index]!, rightId)
    segmentIds.push(rightId)
  }
  let selectedClipId = clipId
  if (action === 'discard') {
    selectedClipId = segmentIds[rows.findIndex((row) => !discardedRowIds.includes(row.id))]!
    for (let index = 0; index < rows.length; index++) if (discardedRowIds.includes(rows[index]!.id)) result = deleteClip(result, segmentIds[index]!)
  }
  assertValidComposition(result)
  return { document: result, selectedClipId }
}
