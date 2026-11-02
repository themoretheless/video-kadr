import { keyframeLocalTime } from '../composition/keyframeClock'
import {describeHistoryStacks} from './historyNavigation'
import {startAgentActivity,finishAgentActivity,linkActivityCheckpoint,type ActivityToken} from './activity.svelte'
import { restyleCaptions, type CaptionStylePatch } from '../composition/restyleCaptions'
import { nestedComposition,nestedHistory } from '../state/nestedComposition.svelte'
import { mergeCaptions } from '../composition/mergeCaptions'
import { captionExportCues, nestedCaptionExportCues, formatSubtitleExport } from '../subtitles/export'
import { findClipLocation } from '../composition/commands'
import { evaluateAgentBatch, type AgentOperation } from './operations.js'
import { agentSession, assertHistoryUnlocked, beginAgentWork, endAgentWork } from './session.svelte.js'
import { cloneComposition, cloneCompositionMedia, commitDocument, compositionRenderOutput, compositionState, makeId } from '../state/composition/core.svelte.js'
import { jumpCompositionHistory,undoComposition, redoComposition } from '../state/composition/transport.svelte.js'
import { resolveAutomation, type CompositionAutomationProperty, type CompositionAutomationTarget } from '../state/composition/core.svelte.js'
import { COMPOSITION_VISUAL_PROPERTIES, COMPOSITION_AUDIO_PROPERTIES, COMPOSITION_MASK_PROPERTIES, COMPOSITION_TIME_BASE } from '../composition/types.js'
import { resolvedKeyframeTangents, segmentInterpolation } from '../composition/tangents.js'
import { easingPresetId } from '../composition/easingPresets.js'
import { colorToNative,resolvedColorTangents } from '../composition/colorAnimation'

export function executeAgentTool(actor: string, tool: string, args: Record<string, unknown>): unknown {
  const activity=startAgentActivity(actor,tool,args,agentSession.work)
  try {const result=executeAgentToolInternal(actor,tool,args,activity);finishAgentActivity(activity,undefined,result);return result}
  catch(error){finishAgentActivity(activity,error,undefined,true);throw error}
}
function executeAgentToolInternal(actor: string, tool: string, args: Record<string, unknown>, activity?:ActivityToken): unknown {
  if (!actor || actor.length > 128) throw new Error('Invalid actor')
  const id = makeId('agent-op')
  logAgentOperation(id, actor, tool, 'started')
  try {
    const result = run(actor, tool, args,activity)
    logAgentOperation(id, actor, tool, 'ok')
    return result
  } catch (error) {
    logAgentOperation(id, actor, tool, 'error', error instanceof Error ? error.message : String(error))
    throw error
  }
}

function run(actor: string, tool: string, args: Record<string, unknown>, activity?:ActivityToken): unknown {
  switch (tool) {
    case 'begin_agent_session': return beginAgentWork(actor,args.reason,'ordinary',label=>run(actor,'create_checkpoint',{label},activity) as {id:string})
    case 'end_agent_session': return endAgentWork(actor,args.force??false)

    case 'restyle_captions': {
      if(args.expectedProjectHash!==undefined)throw new Error('expectedProjectHash requires an open nested project')
      if (nestedComposition.document) throw new Error('Use project caption styling in the nested editor; MCP revision admission currently covers ordinary compositions')
      if (args.expectedRevision !== agentSession.revision) throw new Error('Project changed; read get_project and retry with its revision')
      assertHistoryUnlocked(actor)
      const {expectedRevision, layer_ids, ...patch} = args
      void expectedRevision
      const result = restyleCaptions(compositionState.document,patch as CaptionStylePatch,layer_ids as string[] | undefined)
      commitDocument(result.document,undefined,undefined,'validated-batch',activity)
      return {captions:result.captions,restyled:result.restyled,revision:agentSession.revision}
    }

    case 'merge_captions': {
      if(args.expectedProjectHash!==undefined||args.composition_id!=null)throw new Error('Nested caption guards require an open nested project')
      if (nestedComposition.document) throw new Error('Use nested caption merge in the editor; MCP revision admission currently covers ordinary compositions')
      if (args.expectedRevision !== agentSession.revision) throw new Error('Project changed; read get_project and retry with its revision')
      assertHistoryUnlocked(actor)
      const result = mergeCaptions(compositionState.document, args.layer_ids as string[])
      commitDocument(result.document, undefined, undefined, 'validated-batch',activity)
      return { ...result.record, revision: agentSession.revision }
    }

    case 'export_captions': {
      if (args.format !== 'srt' && args.format !== 'vtt') throw new Error('format must be srt or vtt')
      const trackId = args.track_id == null ? undefined : text(args.track_id, 'track_id', 128)
      const compositionId = args.composition_id == null ? undefined : text(args.composition_id, 'composition_id', 128)
      if (!nestedComposition.document && compositionId) throw new Error('composition_id requires an open nested project')
      const cues = nestedComposition.document
        ? nestedCaptionExportCues(nestedComposition.document,compositionId,trackId)
        : captionExportCues(compositionState.document,trackId)
      return { format: args.format, cues: cues.length, body: formatSubtitleExport(cues, args.format) }
    }

    case 'get_color_track': {
      if(args.composition_id!==undefined)throw new Error('composition_id requires an open nested project')
      const { clip } = findClipLocation(compositionState.document,text(args.clipId,'clipId',128))
      if (clip.kind !== 'text' && clip.kind !== 'color') throw new Error('Color track requires a text or color layer')
      const value = clip.colorAnimation ?? { mode: 'constant' as const,value: clip.kind === 'text' ? clip.style.color : clip.color }
      if (value.mode === 'constant') return { revision: agentSession.revision,mode: 'constant',value: colorToNative(value.value) }
      const track = value.track
      return { revision: agentSession.revision,mode: 'keyframes',timeBase: track.timeBase,interpolation: JSON.parse(JSON.stringify(track.interpolation)),extrapolation: track.extrapolation ? { ...track.extrapolation } : undefined,keyframes: track.keyframes.map((key,index) => {
        const next = track.keyframes[index+1]
        const segment = next ? segmentInterpolation({ ...key,value: 0 },{ ...next,value: 1 },track.interpolation) : undefined
        const preset = segment ? easingPresetId(segment) : undefined
        const tangents = resolvedColorTangents(value,index)
        return { tick: key.tick,t_local_us: key.tick*COMPOSITION_TIME_BASE/track.timeBase,t_us: clip.timelineStartTicks+key.tick*COMPOSITION_TIME_BASE/track.timeBase,value: colorToNative(key.value),incoming: { ...tangents.incoming },outgoing: { ...tangents.outgoing },continuity: tangents.continuity,...(segment ? { segment: JSON.parse(JSON.stringify(segment)) } : {}),...(preset ? { preset_id: preset } : {}) }
      }) }
    }
    case 'get_keyframes': {
      if(args.composition_id!==undefined)throw new Error('composition_id requires an open nested project')
      const clipId = text(args.clipId,'clipId',128)
      const envelope = args.envelope ?? 'visual'
      if (envelope !== 'visual' && envelope !== 'audio' && envelope !== 'mask' && envelope !== 'path') throw new Error('Unknown animation envelope')
      const properties = envelope === 'path' ? ['progress'] : envelope === 'visual' ? COMPOSITION_VISUAL_PROPERTIES : envelope === 'audio' ? COMPOSITION_AUDIO_PROPERTIES : COMPOSITION_MASK_PROPERTIES
      if (!properties.some(property => property === args.property)) throw new Error('Unknown animation property')
      const [clip,current,fallback] = (() => {
        if (envelope === 'path') {
          const { clip } = findClipLocation(compositionState.document,clipId)
          if (clip.kind === 'audio' || clip.position?.mode !== 'path') throw new Error('Path progress required')
          return [clip,clip.position.progress,0] as const
        }
        const target: CompositionAutomationTarget = envelope === 'mask' ? { kind: 'mask',clipId,maskId: text(args.maskId,'maskId',128) } : { kind: envelope,clipId }
        return resolveAutomation(target,args.property as CompositionAutomationProperty)
      })()
      if (!current || current.mode === 'constant') return { revision: agentSession.revision,mode: 'constant',value: current?.value ?? fallback }
      const track = current.track
      return { revision: agentSession.revision,mode: 'keyframes',timeBase: track.timeBase,...(track.sampleClock ? { sampleClock: { ...track.sampleClock } } : {}),interpolation: JSON.parse(JSON.stringify(track.interpolation)),extrapolation: track.extrapolation ? { ...track.extrapolation } : undefined,
        keyframes: track.keyframes.map((key,index) => {
          const next = track.keyframes[index+1]
          const segment = next ? segmentInterpolation(key,next,track.interpolation) : undefined
          const preset = segment ? easingPresetId(segment) : undefined
          return { tick: key.tick,t_local_us: keyframeLocalTime(track,key.tick),t_us: clip.timelineStartTicks+keyframeLocalTime(track,key.tick),value: key.value,...resolvedKeyframeTangents(track.keyframes,index,track.interpolation),...(segment ? { segment: JSON.parse(JSON.stringify(segment)) } : {}),...(preset ? { preset_id: preset } : {}) }
        }),
      }
    }
    case 'get_project': return {
      workSession:agentSession.work?{...agentSession.work}:null,agentView:agentSession.view,
      projectId: compositionState.projectId,
      name: compositionState.projectName,
      revision: agentSession.revision,
      document: cloneComposition(compositionState.document),
    }
    case 'get_history': return {
      revision:agentSession.revision,
      ...describeHistoryStacks(compositionState.history.past,compositionState.history.future),
      undoCount: compositionState.history.past.length,
      redoCount: compositionState.history.future.length,
      lock: agentSession.lock,
      checkpoints: agentSession.checkpoints.map(({ id, label, createdAt }) => ({ id, label, createdAt })),
    }
    case 'dry_run':
    case 'apply_batch': {
      if(args.expectedProjectHash!==undefined||args.composition_id!==undefined)throw new Error('expectedProjectHash/composition_id requires an open nested project')
      if (args.expectedRevision !== agentSession.revision) throw new Error('Project changed; read get_project and retry with its revision')
      assertHistoryUnlocked(actor)
      const document = evaluateAgentBatch(compositionState.document, args.operations as AgentOperation[])
      if (tool === 'apply_batch') commitDocument(document, undefined, undefined, 'validated-batch',activity)
      return { revision: agentSession.revision, dryRun: tool === 'dry_run', document }
    }
    case 'list_checkpoints': return agentSession.checkpoints.map(item=>({id:item.id,label:item.label,actor:item.actor??'user',created_at:new Date(item.createdAt).toISOString()}))
    case 'create_checkpoint': {
      const label = text(args.label, 'label', 128)
      if (agentSession.checkpoints.length >= 20) throw new Error('Checkpoint limit is 20; delete a checkpoint first')
      const checkpoint = { id: makeId('checkpoint'), label, actor, createdAt: Date.now(), document: cloneComposition(compositionState.document), media: cloneCompositionMedia(compositionState.media), output: compositionRenderOutput() }
      agentSession.checkpoints.push(checkpoint)
      linkActivityCheckpoint(activity,checkpoint.id,checkpoint.label,'checkpoint')
      return { id: checkpoint.id, label, createdAt: checkpoint.createdAt }
    }
    case 'delete_checkpoint': {
      const id = text(args.id, 'id', 128)
      if (!agentSession.checkpoints.some((item) => item.id === id)) throw new Error('Checkpoint not found')
      agentSession.checkpoints = agentSession.checkpoints.filter((item) => item.id !== id)
      return { deleted: id }
    }
    case 'restore_checkpoint': {
      if(args.expectedProjectHash!==undefined)throw new Error('expectedProjectHash requires an open nested project')
      assertHistoryUnlocked(actor)
      const checkpoint = agentSession.checkpoints.find((item) => item.id === args.id)
      if (!checkpoint) throw new Error('Checkpoint not found')
      commitDocument(checkpoint.document, checkpoint.media, checkpoint.output, 'restore',activity)
      linkActivityCheckpoint(activity,checkpoint.id,checkpoint.label,'restore')
      return { revision: agentSession.revision }
    }
    case 'set_history_lock': {
      assertHistoryUnlocked(actor)
      if (typeof args.locked !== 'boolean') throw new Error('locked must be boolean')
      if (!args.locked) agentSession.lock = null
      else agentSession.lock = { owner: actor, reason: text(args.reason, 'reason', 256), expiresAt: Date.now() + 60_000 }
      return { lock: agentSession.lock }
    }
    case 'jump_history':{
      if(Object.keys(args).some(field=>!['entryId','side','expectedRevision'].includes(field)))throw new Error('Invalid ordinary history jump argument')
      if(args.expectedRevision!==agentSession.revision)throw new Error('Project changed; read get_project again')
      assertHistoryUnlocked(actor)
      const side=args.side??'after'
      if(side!=='before'&&side!=='after')throw new Error('Invalid history jump side')
      const steps=jumpCompositionHistory(text(args.entryId,'entryId',128),side,actor)
      return {revision:agentSession.revision,steps,...describeHistoryStacks(compositionState.history.past,compositionState.history.future)}
    }
    case 'undo': undoComposition(actor); return { revision: agentSession.revision }
    case 'redo': redoComposition(actor); return { revision: agentSession.revision }
    default: throw new Error(`Unknown tool: ${tool}`)
  }
}

function text(value: unknown, name: string, max: number): string {
  if (typeof value !== 'string' || !value.trim() || value.length > max) throw new Error(`Invalid ${name}`)
  return value.trim()
}

export function logAgentOperation(id: string, actor: string, tool: string, status: 'started' | 'ok' | 'error', detail?: string): void {
  agentSession.log.push({ id, actor, tool, status, time: Date.now(), ...(detail ? { detail: detail.slice(0, 512) } : {}) })
  if (agentSession.log.length > 200) agentSession.log.splice(0, agentSession.log.length - 200)
}


export async function executeAgentRequest(actor: string, tool: string, args: Record<string, unknown>, access: AgentAccess = {}): Promise<unknown> {
  const activity=startAgentActivity(actor,tool,args,agentSession.work)
  try {
    const result=await dispatchAgentRequest(actor,tool,args,{...access,activity})
    const partialFailure=tool==='command_batch'&&result!==null&&typeof result==='object'&&'failed' in result&&typeof result.failed==='number'&&result.failed>0
    finishAgentActivity(activity,partialFailure?new Error('Some command batch steps failed'):undefined,result,partialFailure)
    return result
  }
  catch(error){finishAgentActivity(activity,error,undefined,true);throw error}
}
async function dispatchAgentRequest(actor: string, tool: string, args: Record<string, unknown>, access: AgentAccess): Promise<unknown> {
  if (tool === 'command_list' || tool === 'command_run' || tool === 'command_batch') {
    const { executeFilmcraftCommand } = await import('./filmcraftCommands')
    return executeFilmcraftCommand(actor, tool, args, access)
  }
  if ((nestedComposition.document||tool==='redo'&&nestedHistory.future.length>0) && !['export_captions','set_history_lock'].includes(tool)) {
    const id=makeId('agent-op')
    logAgentOperation(id,actor,tool,'started')
    try {
      const {executeNestedCaptionRequest}=await import('./nestedCaptions.svelte')
      const result=await executeNestedCaptionRequest(actor,tool,args,access)
      logAgentOperation(id,actor,tool,'ok')
      return result
    } catch(error) {
      logAgentOperation(id,actor,tool,'error',error instanceof Error?error.message:String(error))
      throw error
    }
  }

  if (['analyze_clip', 'measure_shots', 'compare_frames', 'auto_split_by_shot', 'mark_shots', 'apply_shot_cuts'].includes(tool)) {
    const { executeAgentCompute } = await import('./compute.svelte.js')
    return executeAgentCompute(actor, tool, args, access)
  }
  return executeAgentToolInternal(actor, tool, args,access.activity)
}

export interface AgentAccess { activity?:ActivityToken; token?: string | null; spaceId?: string | null; expiresAt?: number; isActive?: () => boolean }
