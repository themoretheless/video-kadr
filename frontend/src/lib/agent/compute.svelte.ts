import { scanLibraryShots, measureLibraryShots, compareLibraryFrames } from '../api.js'
import { findClipLocation } from '../composition/commands.js'
import { cloneComposition, compositionState, commitDocument, makeId, selectCompositionClip } from '../state/composition/core.svelte.js'
import { agentSession, assertHistoryUnlocked } from './session.svelte.js'
import { logAgentOperation, type AgentAccess } from './runtime.svelte.js'
import { applyShotReview, reviewShotRows, reviewUnplacedShots } from '../analysis/shots.js'
import { measurementKey, shotAnalysisState } from '../analysis/shotState.svelte.js'

export async function executeAgentCompute(actor: string, tool: string, args: Record<string, unknown>, access: AgentAccess): Promise<unknown> {
  if (!actor || actor.length > 128) throw new Error('Invalid actor')
  const operationId = makeId('agent-compute')
  logAgentOperation(operationId, actor, tool, 'started')
  try {
    const result = await compute(actor, tool, args, access)
    logAgentOperation(operationId, actor, tool, 'ok')
    return result
  } catch (error) {
    logAgentOperation(operationId, actor, tool, 'error', error instanceof Error ? error.message : String(error))
    throw error
  }
}
async function compute(actor: string, tool: string, args: Record<string, unknown>, access: AgentAccess): Promise<unknown> {
  const editing = ['auto_split_by_shot', 'mark_shots', 'apply_shot_cuts'].includes(tool)
  const clipId = args.clipId, mediaId = args.media_id
  if (editing && (typeof clipId !== 'string' || mediaId !== undefined)) throw new Error('Editing requires clipId')
  if (!editing && ((clipId === undefined) === (mediaId === undefined) ||
    (clipId !== undefined && (typeof clipId !== 'string' || !clipId.length)) ||
    (mediaId !== undefined && (typeof mediaId !== 'string' || !mediaId.length || mediaId.length > 128)))) {
    throw new Error('Specify exactly one clipId or library media_id')
  }
  if (args.ripple !== undefined && (tool !== 'apply_shot_cuts' || typeof args.ripple !== 'boolean')) {
    throw new Error('ripple must be a boolean on apply_shot_cuts')
  }
  if (args.ripple === true && (!Array.isArray(args.discardedRowIds) || !args.discardedRowIds.length)) {
    throw new Error('ripple requires at least one discarded scene')
  }
  const baseline = cloneComposition(compositionState.document)
  const revision = agentSession.revision
  const location = typeof clipId === 'string' ? findClipLocation(baseline, clipId) : undefined
  if (location && location.clip.kind !== 'video') throw new Error('Shot analysis requires a video clip')
  const clip = location?.clip.kind === 'video' ? location.clip : undefined
  const sourceId = clip?.sourceId ?? mediaId as string
  if (tool === 'compare_frames') {
    const left = args.leftTick
    const right = args.rightTick
    if (typeof left !== 'number' || typeof right !== 'number' || !Number.isSafeInteger(left) || !Number.isSafeInteger(right) ||
        left < (clip?.sourceInTicks ?? 0) || right < (clip?.sourceInTicks ?? 0) ||
        (clip && (left >= clip.sourceOutTicks || right >= clip.sourceOutTicks))) throw new Error('Frame ticks must be inside the source window')
    return { revision, sourceId, ...await compareLibraryFrames(sourceId, left, right, access.token, access.spaceId) }
  }
  if (editing) {
    if (args.expectedRevision !== revision) throw new Error('Project changed; read get_project before applying cuts')
    assertHistoryUnlocked(actor)
    if (location?.track.locked) throw new Error('Track is locked')
  }
  const scan = await scanLibraryShots(sourceId, access.token, access.spaceId)
  shotAnalysisState.scans[sourceId] = scan
  const threshold = args.threshold === undefined ? baseline.shotReview?.threshold ?? 0.3 : args.threshold
  const minimum = args.minShotTicks === undefined ? baseline.shotReview?.minShotTicks ?? 500_000 : args.minShotTicks
  if (typeof threshold !== 'number' || typeof minimum !== 'number') throw new Error('Invalid review parameters')
  const disabled = args.disabledCuts ?? []
  const discarded = args.discardedRowIds ?? []
  if (!Array.isArray(disabled) || disabled.some((tick) => !Number.isSafeInteger(tick)) || !Array.isArray(discarded) || discarded.some((id) => typeof id !== 'string')) throw new Error('Invalid review selection')
  const placedRows = clip ? reviewShotRows(baseline, clip.id, scan, threshold, minimum, disabled) : undefined
  const rows = placedRows ?? reviewUnplacedShots(scan, threshold, minimum, disabled)
  const measurements = []
  if (tool === 'measure_shots' || args.stats === true) {
    for (let index = 0; index < rows.length; index += 64) {
      const stats = await measureLibraryShots(sourceId, rows.slice(index, index + 64).map(({ startTicks, endTicks }) => ({ startTicks, endTicks })), access.token, access.spaceId, scan.sourceHash)
      for (const item of stats) shotAnalysisState.measurements[measurementKey(scan, item.span.startTicks, item.span.endTicks)] = item
      measurements.push(...stats)
    }
  }
  if (editing && clip && location && placedRows) {
    if ((access.expiresAt !== undefined && Date.now() >= access.expiresAt) || (access.isActive && !access.isActive())) throw new Error('Command expired or editor disconnected; no edit was committed')
    if (agentSession.revision !== revision) throw new Error('Project changed during analysis; no edit was committed')
    assertHistoryUnlocked(actor)
    const action = tool === 'mark_shots' ? 'mark' : args.ripple === true ? 'ripple-discard' : discarded.length ? 'discard' : 'split'
    const result = applyShotReview(baseline, clip.id, placedRows, action, discarded, () => makeId(action === 'mark' ? 'shot-marker' : 'shot-segment'))
    commitDocument(result.document,undefined,undefined,'content',access.activity)
    selectCompositionClip(location.track.id, result.selectedClipId)
    return { revision: agentSession.revision, selectedClipId: result.selectedClipId, rows }
  }
  return { revision, sourceId, scan, rows, measurements }
}
