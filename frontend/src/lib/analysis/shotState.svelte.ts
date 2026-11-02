import { agentSession, bumpAgentRevision } from '../agent/session.svelte.js'
import { compositionState, commitDocument, makeId, scheduleAutosave, selectCompositionClip } from '../state/composition/core.svelte.js'
import { findClipLocation } from '../composition/commands.js'
import { applyShotReview, reviewShotRows, type ShotMeasurement, type ShotScan } from './shots.js'

export const shotAnalysisState = $state({ scans: {} as Record<string, ShotScan>, measurements: {} as Record<string, ShotMeasurement> })
export function measurementKey(scan: ShotScan, start: number, end: number): string { return `${scan.sourceHash}:${start}:${end}` }

/** Review parameters are project setup: fan out without adding an Undo entry. */
export function setShotReviewSettings(threshold: number, minShotTicks: number): void {
  if (!Number.isFinite(threshold) || threshold < 0.05 || threshold > 1 || !Number.isSafeInteger(minShotTicks) || minShotTicks < 1 || minShotTicks > 60_000_000) throw new Error('Invalid shot review settings')
  const patch = (document: typeof compositionState.document) => ({ ...document, shotReview: { threshold, minShotTicks } })
  compositionState.document = patch(compositionState.document)
  compositionState.history.past = compositionState.history.past.map(patch)
  compositionState.history.future = compositionState.history.future.map(patch)
  agentSession.checkpoints = agentSession.checkpoints.map((checkpoint) => ({ ...checkpoint, document: patch(checkpoint.document) }))
  bumpAgentRevision()
  scheduleAutosave()
}

export function applyCompositionShotReview(clipId: string, threshold: number, minShotTicks: number, disabledCuts: readonly number[], action: 'split' | 'mark' | 'discard', discardedIds: readonly string[]): void {
  const { clip, track } = findClipLocation(compositionState.document, clipId)
  if (clip.kind !== 'video') throw new Error('Выберите видеоклип')
  const scan = shotAnalysisState.scans[clip.sourceId]
  if (!scan) throw new Error('Сначала выполните анализ сцен')
  const rows = reviewShotRows(compositionState.document, clip.id, scan, threshold, minShotTicks, disabledCuts)
  const result = applyShotReview(compositionState.document, clipId, rows, action, discardedIds, () => makeId(action === 'mark' ? 'shot-marker' : 'shot-segment'))
  commitDocument(result.document)
  selectCompositionClip(track.id, result.selectedClipId)
}
