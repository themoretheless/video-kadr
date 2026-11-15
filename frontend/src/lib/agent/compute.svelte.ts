import { scanLibraryShots, measureLibraryShots, compareLibraryFrames, describeLibraryShot, getSceneDescriptionCatalog } from '../api.js'
import { findClipLocation } from '../composition/commands.js'
import { cloneComposition, compositionState, commitDocument, makeId, selectCompositionClip } from '../state/composition/core.svelte.js'
import { parseSceneDescriptionLanguage } from '../analysis/sceneDescriptionLanguages.js'
import { sceneDescriptionPreferences, type SceneDescriptionPreferences } from '../state/sceneDescriptionPreferences.svelte.js'
import { nestedComposition } from '../state/nestedComposition.svelte.js'
import { agentSession, assertHistoryUnlocked } from './session.svelte.js'
import { logAgentOperation, type AgentAccess } from './runtime.svelte.js'
import { applyShotReview, clipDescriptionSpan, reviewShotRows, reviewUnplacedShots, type ShotSpan } from '../analysis/shots.js'
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
  if (tool === 'read_scene_descriptions') return readSceneDescriptions(args, access)
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
  if (tool === 'describe_clip') {
    if (Object.keys(args).some(key => !['clipId', 'media_id', 'composition_id', 'sourceSpan', 'focus', 'language', 'fps'].includes(key)) ||
        (typeof clipId === 'string' && clipId.length > 128)) {
      throw new Error('Invalid describe_clip arguments')
    }
    const scope = args.composition_id
    const project = nestedComposition.document
    if (scope !== undefined && (!project || typeof clipId !== 'string' ||
        typeof scope !== 'string' || !scope || scope.length > 128)) {
      throw new Error('composition_id requires a nested timeline clip')
    }
    if (project && typeof clipId === 'string') {
      const compositionId = typeof scope === 'string' ? scope : project.root
      const document = project.compositions[compositionId]?.content
      if (!document) throw new Error('Nested composition not found')
      const clip = document.tracks.flatMap(track => track.kind === 'video' ? track.clips : [])
        .find(candidate => candidate.id === clipId)
      if (!clip) throw new Error('Scene description requires a video clip in the requested composition')
      const item = project.projectMedia?.[clip.sourceId]
      const sourceId = item ? item.librarySourceId : clip.sourceId
      if (!sourceId || item?.placeholder) throw new Error('Video clip has no bound library source')
      const revision = nestedComposition.revision
      const span = clipDescriptionSpan({
        sourceInTicks: clip.placement.sourceInTick,
        sourceOutTicks: clip.placement.sourceOutTick,
        playbackMode: clip.playbackMode,
      })
      return { revision, compositionId, ...await describeSource(sourceId, span, args, access) }
    }
  }
  const baseline = cloneComposition(compositionState.document)
  const revision = agentSession.revision
  const location = typeof clipId === 'string' ? findClipLocation(baseline, clipId) : undefined
  if (location && location.clip.kind !== 'video') throw new Error('Shot analysis requires a video clip')
  const clip = location?.clip.kind === 'video' ? location.clip : undefined
  const sourceId = clip?.sourceId ?? mediaId as string
  if (tool === 'describe_clip') {
    return { revision, ...await describeSource(sourceId, clip && clipDescriptionSpan(clip), args, access) }
  }
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
  const remaining = !clip && access.expiresAt !== undefined ? access.expiresAt - Date.now() : undefined
  if (remaining !== undefined && remaining <= 0) throw new Error('Analysis command expired')
  const options: [] | [AbortSignal] = remaining === undefined ? [] : [AbortSignal.timeout(Math.min(remaining, 240_000))]
  const checkSourceCommand = () => {
    if (!clip && (options[0]?.aborted || access.isActive?.() === false)) {
      throw new Error('Analysis command expired or editor disconnected')
    }
  }
  checkSourceCommand()
  const scan = await scanLibraryShots(sourceId, access.token, access.spaceId, ...options)
  checkSourceCommand()
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
      const stats = await measureLibraryShots(sourceId, rows.slice(index, index + 64).map(({ startTicks, endTicks }) => ({ startTicks, endTicks })), access.token, access.spaceId, scan.sourceHash, ...options)
      checkSourceCommand()
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

async function describeSource(sourceId: string, clipSpan: ShotSpan | undefined, args: Record<string, unknown>, access: AgentAccess) {
  if (!access.token) throw new Error('Scene description requires an authenticated editor')
  const { focus, language, fps } = descriptionView(args)

  const supplied = args.sourceSpan
  let requestedSpan: ShotSpan | undefined
  if (supplied !== undefined) {
    if (typeof supplied !== 'object' || supplied === null || Array.isArray(supplied)) throw new Error('Invalid sourceSpan')
    const { startTicks, endTicks } = supplied as Partial<ShotSpan>
    if (Object.keys(supplied).length !== 2 || typeof startTicks !== 'number' || typeof endTicks !== 'number' ||
        !Number.isSafeInteger(startTicks) || !Number.isSafeInteger(endTicks) || startTicks < 0 || endTicks <= startTicks) {
      throw new Error('Invalid sourceSpan')
    }
    if (clipSpan && (startTicks < clipSpan.startTicks || endTicks > clipSpan.endTicks)) {
      throw new Error('Description span must be inside the clip source range')
    }
    requestedSpan = { startTicks, endTicks }
  }

  const remaining = access.expiresAt === undefined ? undefined : access.expiresAt - Date.now()
  if ((remaining !== undefined && remaining <= 0) || access.isActive?.() === false) {
    throw new Error('Description command expired or editor disconnected')
  }
  const signal = remaining === undefined ? undefined : AbortSignal.timeout(Math.min(remaining, 420_000))
  const scan = await scanLibraryShots(sourceId, access.token, access.spaceId, signal)
  const span = requestedSpan ?? clipSpan ?? { startTicks: 0, endTicks: scan.durationTicks }
  if (span.endTicks > scan.durationTicks) throw new Error('Description span must be inside the source duration')
  if (signal?.aborted || access.isActive?.() === false) throw new Error('Description command expired or editor disconnected')
  const description = await describeLibraryShot(sourceId, {
    span, expectedSourceHash: scan.sourceHash, focus, language, fps,
  }, access.token, access.spaceId, signal)
  if (signal?.aborted || access.isActive?.() === false) throw new Error('Description command expired or editor disconnected')
  return { sourceId, span, focus, language, fps, ...description }
}

async function readSceneDescriptions(args: Record<string, unknown>, access: AgentAccess) {
  const { media_id: sourceId, startTicks, endTicks, cursor } = args
  if (!access.token) throw new Error('Scene description requires an authenticated editor')
  if (typeof sourceId !== 'string' || !sourceId || sourceId.length > 128 ||
      Object.keys(args).some(key => !['media_id', 'focus', 'language', 'startTicks', 'endTicks', 'cursor', 'fps'].includes(key))) {
    throw new Error('Invalid scene description source')
  }
  const { focus, language, fps } = descriptionView(args)
  let span: ShotSpan | undefined
  if (startTicks !== undefined || endTicks !== undefined) {
    if (typeof startTicks !== 'number' || typeof endTicks !== 'number' ||
      !Number.isSafeInteger(startTicks) || !Number.isSafeInteger(endTicks) || startTicks < 0 || endTicks <= startTicks ||
      cursor !== undefined) throw new Error('Specify a valid source span or a cursor')
    span = { startTicks, endTicks }
  }
  if (cursor !== undefined && (typeof cursor !== 'string' || !/^[0-9]{1,20}-[0-9]{1,20}$/.test(cursor))) {
    throw new Error('Invalid scene description cursor')
  }
  const remaining = access.expiresAt === undefined ? 120_000 : access.expiresAt - Date.now()
  if (remaining <= 0 || access.isActive?.() === false) throw new Error('Description command expired or editor disconnected')
  const signal = AbortSignal.timeout(Math.min(remaining, 120_000))
  const result = await getSceneDescriptionCatalog(sourceId, { focus, language, fps, ...span, cursor },
    access.token, access.spaceId, signal)
  if (signal.aborted || access.isActive?.() === false) throw new Error('Description command expired or editor disconnected')
  return { sourceId, focus, language, fps, ...result }
}

function descriptionView(args: Record<string, unknown>): SceneDescriptionPreferences {
  const focus = args.focus === undefined ? sceneDescriptionPreferences.focus : args.focus
  const language = args.language === undefined ? sceneDescriptionPreferences.language : parseSceneDescriptionLanguage(args.language)
  const fps = args.fps === undefined ? sceneDescriptionPreferences.fps : args.fps
  if (focus !== 'general' && focus !== 'shot-type') throw new Error('Invalid scene description focus')
  if (language === undefined) throw new Error('Invalid scene description language')
  if (typeof fps !== 'number' || !Number.isFinite(fps) || fps <= 0 || fps > 30) {
    throw new Error('fps must be greater than 0 and at most 30')
  }
  return { focus, language, fps: Math.max(1, Math.round(fps * 1000)) / 1000 }
}
