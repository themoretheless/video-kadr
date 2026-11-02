import { setMotionPathProgressKey,switchStaticPositionMode } from '../composition/motionPathCommands'
import { normalizeColorAnimation } from '../composition/colorAnimation'
import { nativeColorToHex } from '../composition/colorAnimation'
import { setTextColorKey,deleteTextColorKey,configureTextColor,setTextColorTangentMode,editTextColorTangents,updateTextColorKey,moveTextColorKeys } from '../composition/colorCommands'
import { normalizeAuthoredTangentAnimation, setKeyframeTangentMode, editKeyframeTangent, updateKeyframeTangents, normalizeKeyframeTangents, cloneKeyframe } from '../composition/tangents.js'
import { EASING_PRESETS } from '../composition/easingPresets.js'
import { updateSelectedKeyframeInterpolation, upsertKeyframe, deleteKeyframe } from '../composition/keyframes.js'
import type { CompositionVisualProperty, CompositionAudioProperty, CompositionMaskProperty, CompositionAnimatableValue } from '../composition/types.js'
import { COMPOSITION_VISUAL_PROPERTIES, COMPOSITION_AUDIO_PROPERTIES, COMPOSITION_MASK_PROPERTIES } from '../composition/types.js'
import { patchCompositionCanvas } from '../composition/canvas.js'
import { moveClipsToNewTrack, moveClipPlacements } from '../composition/commands.js'
import { assertCompositionLocksPreserved } from '../composition/locks.js'
import { rippleDeleteGap, rippleDeleteClips } from '../composition/timelineProductivity.js'
import { slipClip,setClipLocked, setClipsEnabled, moveClips, pasteClips, duplicateClip, addClip, addTrack, deleteClip, deleteTrack, deleteTransition, findClipLocation, moveClip, reorderTrack, splitClip, trimClip, upsertTransition } from '../composition/commands.js'
import { upsertCompositionMarker, deleteCompositionMarker, type CompositionMarker } from '../composition/markers.js'
import type { Composition, CompositionClip, CompositionTrack, CompositionTransition } from '../composition/types.js'
import { assertValidComposition } from '../composition/validation.js'
import { createCompositionLink, removeCompositionLink } from '../composition/links.js'

export type AgentOperation =
  | {op:'slip_layer';clipId:string;sourceDeltaTicks:number}
  | {op:'set_keyframe_extrapolation';clipId:string;property:CompositionVisualProperty|CompositionAudioProperty|CompositionMaskProperty|'progress';envelope?:'visual'|'audio'|'mask'|'path';maskId?:string;side:'before'|'after';extrapolation:import('../composition/types').CompositionExtrapolationMode}
  | { op: 'move_color_keys';edits: readonly { clipId: string;tick: number;newTick: number }[] }
  | { op: 'set_color_key';clipId: string;playheadTicks: number;value: import('../api').NativeColor }
  | { op: 'edit_color_key';clipId: string;tick: number;newTick?: number;value?: import('../api').NativeColor }
  | { op: 'delete_color_key';clipId: string;tick: number }
  | { op: 'set_color_interpolation';clipId: string;interpolation: import('../composition/types').CompositionInterpolation;tick?: number }
  | { op: 'set_color_preset';clipId: string;preset: string;tick?: number }
  | { op: 'set_color_extrapolation';clipId: string;side: 'before' | 'after';extrapolation: import('../composition/types').CompositionExtrapolationMode }
  | { op: 'set_color_tangent_mode';clipId: string;tick: number;mode: 'auto' | 'free' }
  | { op: 'edit_color_tangents';clipId: string;tick: number;continuity?: 'smooth' | 'broken';side?: 'incoming' | 'outgoing';point?: { x: number;y: number } }
  | { op: 'set_path_progress';clipId: string;playheadTicks: number;value: number }
  | { op: 'switch_position_mode';clipId: string;positionMode: 'xy' | 'path';nodeId: string }
  | { op: 'edit_keyframe'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; tick: number; newTick?: number; value?: number; preset?: string; incoming?: { x: number;y: number }; outgoing?: { x: number;y: number }; continuity?: 'smooth' | 'broken' }
  | { op: 'upsert_keyframe'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; tick: number; value: number; preset?: string }
  | { op: 'delete_keyframe'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; tick: number }
  | { op: 'move_keyframe'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; tick: number; newTick: number }
  | { op: 'edit_keyframe_tangents'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; tick: number; incoming?: { x: number;y: number }; outgoing?: { x: number;y: number }; continuity?: 'smooth' | 'broken' }
  | { op: 'set_keyframe_tangent_mode'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; ticks: number[]; mode: 'auto' | 'free' }
  | { op: 'set_keyframe_preset'; clipId: string; property: CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty | 'progress'; envelope?: 'visual' | 'audio' | 'mask' | 'path'; maskId?: string; ticks: number[]; preset: string }
  | { op: 'update_canvas'; patch: Partial<Composition['canvas']> }
  | { op: 'set_layer_locked'; clipId: string; locked: boolean }
  | { op: 'set_layers_enabled'; clipIds: string[]; enabled: boolean }
  | { op: 'ripple_delete_gap'; trackId: string; startTicks: number; endTicks: number }
  | { op: 'ripple_delete_layers'; clipIds: string[] }
  | { op: 'move_layers'; clipIds: string[]; trackId: string; startTicks: number }
  | { op: 'move_layer_placements'; placements: { clipId: string; trackId: string }[]; anchorId: string; startTicks: number }
  | { op: 'move_layers_to_new_track'; clipIds: string[]; trackId: string; anchor?: { clipId: string; startTicks: number } }
  | { op: 'paste_layers'; subjects: { clipId: string; id: string }[]; trackId?: string; startTicks?: number; linkIds?: string[] }
  | { op: 'duplicate_layer'; clipId: string; id: string; trackId?: string; startTicks?: number; escapeLink?: boolean; linkId?: string }
  | { op: 'create_link'; id: string; memberIds: string[]; reassign?: boolean }
  | { op: 'remove_link'; id: string }
  | { op: 'add_track'; track: CompositionTrack; index?: number }
  | { op: 'delete_track'; trackId: string }
  | { op: 'reorder_track'; trackId: string; index: number }
  | { op: 'add_layer'; trackId: string; clip: CompositionClip }
  | { op: 'delete_layer'; clipId: string }
  | { op: 'update_layer'; clipId: string; patch: Record<string, unknown> }
  | { op: 'move_layer'; clipId: string; trackId: string; startTicks: number; escapeLink?: boolean }
  | { op: 'trim_layer'; clipId: string; startTicks: number; endTicks: number; escapeLink?: boolean }
  | { op: 'split_layer'; clipId: string; atTicks: number; rightClipId: string; escapeLink?: boolean }
  | { op: 'add_marker'; marker: CompositionMarker }
  | { op: 'remove_marker'; markerId: string }
  | { op: 'add_transition'; trackId: string; transition: CompositionTransition }
  | { op: 'remove_transition'; trackId: string; transitionId: string }

/** Pure command evaluation: a failed batch never touches the live editor. */
export function evaluateAgentBatch(input: Composition, operations: readonly AgentOperation[]): Composition {
  if (!Array.isArray(operations) || !operations.length || operations.length > 100) throw new Error('Batch requires 1–100 operations')
  let document = JSON.parse(JSON.stringify(input)) as Composition
  for (const operation of operations) {
    const previous = document
    if (!operation || typeof operation !== 'object') throw new Error('Invalid operation')
    switch (operation.op) {
      case 'slip_layer':document=slipClip(document,operation.clipId,operation.sourceDeltaTicks);break
      case 'set_color_key': document = setTextColorKey(document,operation.clipId,operation.playheadTicks,nativeColorToHex(operation.value));break
      case 'move_color_keys': document = moveTextColorKeys(document,operation.edits);break
      case 'edit_color_key': {
        if (operation.newTick === undefined && operation.value === undefined) throw new Error('Color key edit requires time or value')
        document = updateTextColorKey(document,operation.clipId,operation.tick,{ ...(operation.newTick !== undefined ? { tick: operation.newTick } : {}),...(operation.value !== undefined ? { value: nativeColorToHex(operation.value) } : {}) });break
      }
      case 'set_color_interpolation': document = configureTextColor(document,operation.clipId,{ interpolation: operation.interpolation,tick: operation.tick });break
      case 'set_color_preset': document = configureTextColor(document,operation.clipId,{ preset: operation.preset,tick: operation.tick });break
      case 'set_color_extrapolation': document = configureTextColor(document,operation.clipId,{ side: operation.side,extrapolation: operation.extrapolation });break
      case 'set_color_tangent_mode': document = setTextColorTangentMode(document,operation.clipId,operation.tick,operation.mode);break
      case 'edit_color_tangents': document = editTextColorTangents(document,operation.clipId,operation.tick,operation);break
      case 'delete_color_key': document = deleteTextColorKey(document,operation.clipId,operation.tick);break
      case 'set_path_progress': document = setMotionPathProgressKey(document,operation.clipId,operation.playheadTicks,operation.value); break
      case 'switch_position_mode':
        if (operation.positionMode !== 'xy' && operation.positionMode !== 'path') throw new Error('Unknown position mode')
        document = switchStaticPositionMode(document,operation.clipId,operation.positionMode,operation.nodeId); break
      case 'edit_keyframe': {
        if (![operation.newTick,operation.value,operation.preset,operation.incoming,operation.outgoing,operation.continuity].some(field => field !== undefined)) throw new Error('Keyframe edit requires a change')
        const subject = { clipId: operation.clipId,property: operation.property,envelope: operation.envelope,maskId: operation.maskId }
        const tick = operation.newTick ?? operation.tick
        const edits: AgentOperation[] = [{ op: 'move_keyframe',...subject,tick: operation.tick,newTick: tick }]
        if (operation.value !== undefined) edits.push({ op: 'upsert_keyframe',...subject,tick,value: operation.value })
        if (operation.preset !== undefined) edits.push({ op: 'set_keyframe_preset',...subject,ticks: [tick],preset: operation.preset })
        if (operation.incoming !== undefined || operation.outgoing !== undefined || operation.continuity !== undefined) edits.push({ op: 'edit_keyframe_tangents',...subject,tick,incoming: operation.incoming,outgoing: operation.outgoing,continuity: operation.continuity })
        document = evaluateAgentBatch(document,edits)
        break
      }
      case 'upsert_keyframe':
      case 'delete_keyframe':
      case 'move_keyframe':
      case 'edit_keyframe_tangents':
      case 'set_keyframe_tangent_mode':
      case 'set_keyframe_extrapolation':
      case 'set_keyframe_preset': {
        const preset = EASING_PRESETS.find(entry => entry.id === operation.preset)
        if ((operation.op === 'set_keyframe_preset' || (operation.op === 'upsert_keyframe' && operation.preset !== undefined)) && !preset) throw new Error('Unknown easing preset')
        if (operation.op === 'set_keyframe_tangent_mode' && operation.mode !== 'auto' && operation.mode !== 'free') throw new Error('Unknown tangent mode')
        const transform = (value: CompositionAnimatableValue): CompositionAnimatableValue => {
          if(operation.op==='set_keyframe_extrapolation'){
            if(value.mode!=='keyframes')throw new Error('Animation track not found')
            if(operation.side!=='before'&&operation.side!=='after'||!['hold','continue','loop','ping_pong','offset'].includes(operation.extrapolation))throw new Error('Invalid extrapolation')
            return {...value,track:{...value.track,extrapolation:{before:'hold',after:'hold',...value.track.extrapolation,[operation.side]:operation.extrapolation}}}
          }
          if (operation.op === 'upsert_keyframe') return upsertKeyframe(value,operation.tick,operation.value,operation.value,preset?.interpolation)
          if (operation.op === 'delete_keyframe') {
            if (value.mode !== 'keyframes' || !value.track.keyframes.some(key => key.tick === operation.tick)) throw new Error('Selected keyframe no longer exists')
            return deleteKeyframe(value,operation.tick)!
          }
          if (operation.op === 'set_keyframe_preset') return updateSelectedKeyframeInterpolation(value,operation.ticks,preset!.interpolation)
          if (operation.op === 'set_keyframe_tangent_mode') return setKeyframeTangentMode(value,operation.ticks,operation.mode)
          if (operation.op === 'move_keyframe') {
            if (value.mode !== 'keyframes' || !Number.isSafeInteger(operation.tick) || !Number.isSafeInteger(operation.newTick) || operation.newTick < 0) throw new Error('Invalid keyframe move')
            if (!value.track.keyframes.some(key => key.tick === operation.tick)) throw new Error('Selected keyframe no longer exists')
            if (operation.tick !== operation.newTick && value.track.keyframes.some(key => key.tick === operation.newTick)) throw new Error('Another key already occupies that exact time')
            const keys = value.track.keyframes.map(key => ({ ...cloneKeyframe(key),tick: key.tick === operation.tick ? operation.newTick : key.tick })).sort((a,b) => a.tick-b.tick)
            return { ...value,track: { ...value.track,keyframes: normalizeKeyframeTangents(keys,value.track.interpolation) } }
          }
          if (!operation.incoming && !operation.outgoing && !operation.continuity) throw new Error('Tangent edit requires a change')
          let changed = value
          for (const side of ['incoming','outgoing'] as const) {
            const point = operation[side]
            if (point) changed = editKeyframeTangent(changed,operation.tick,side,point.x,point.y)
          }
          if (operation.continuity !== undefined) changed = updateKeyframeTangents(changed,[operation.tick],{ continuity: operation.continuity })
          return changed
        }
        const { clip } = findClipLocation(document,operation.clipId)
        if (operation.envelope === 'path') {
          if (clip.kind === 'audio' || clip.position?.mode !== 'path' || operation.property !== 'progress') throw new Error('Path progress required')
          const progress = transform(clip.position.progress)
          document = evaluateAgentBatch(document,[{ op: 'update_layer',clipId: clip.id,patch: { position: { ...clip.position,progress } } }])
          break
        }
        if (operation.envelope === 'audio') {
          if (clip.kind !== 'audio' && clip.kind !== 'video') throw new Error('Audio animation required')
          if (!COMPOSITION_AUDIO_PROPERTIES.includes(operation.property)) throw new Error('Unknown audio property')
          const value = clip.audioAnimation?.[operation.property as CompositionAudioProperty]
          if (!value && operation.op !== 'upsert_keyframe') throw new Error('Animation track not found')
          const audioAnimation = { ...clip.audioAnimation,[operation.property]: transform(value ?? { mode: 'constant',value: operation.value }) }
          document = evaluateAgentBatch(document,[{ op: 'update_layer',clipId: clip.id,patch: { audioAnimation } }])
          break
        }
        if (operation.envelope === 'mask') {
          if (clip.kind !== 'video') throw new Error('Video mask required')
          if (!COMPOSITION_MASK_PROPERTIES.includes(operation.property)) throw new Error('Unknown mask property')
          const mask = clip.masks?.find(entry => entry.id === operation.maskId)
          if (!mask) throw new Error('Mask not found')
          const value = mask[operation.property as CompositionMaskProperty]
          if (!value && operation.op !== 'upsert_keyframe') throw new Error('Animation track not found')
          const changed = transform(value ?? { mode: 'constant',value: operation.value })
          document = evaluateAgentBatch(document,[{ op: 'update_layer',clipId: clip.id,patch: { masks: clip.masks!.map(entry => entry.id === mask.id ? { ...entry,[operation.property]: changed } : entry) } }])
          break
        }
        if (operation.envelope !== undefined && operation.envelope !== 'visual') throw new Error('Unknown animation envelope')
        if (clip.kind === 'audio') throw new Error('Visual animation required')
        if (!COMPOSITION_VISUAL_PROPERTIES.includes(operation.property)) throw new Error('Unknown visual property')
        const value = clip.animation?.[operation.property as CompositionVisualProperty]
        if (!value && operation.op !== 'upsert_keyframe') throw new Error('Animation track not found')
        const changed = transform(value ?? { mode: 'constant',value: operation.value })
        const animation = { ...clip.animation,[operation.property]: changed }
        if (clip.scaleLinked && (operation.property === 'scaleX' || operation.property === 'scaleY')) {
          animation.scaleX = changed
          animation.scaleY = structuredClone(changed)
        }
        document = evaluateAgentBatch(document,[{ op: 'update_layer',clipId: clip.id,patch: { animation } }])
        break
      }
      case 'update_canvas': document = patchCompositionCanvas(document, operation.patch); break
      case 'set_layer_locked': document = setClipLocked(document, operation.clipId, operation.locked); break
      case 'set_layers_enabled': document = setClipsEnabled(document, operation.clipIds, operation.enabled); break
      case 'ripple_delete_gap': document = rippleDeleteGap(document, operation.trackId, operation.startTicks, operation.endTicks); break
      case 'ripple_delete_layers': document = rippleDeleteClips(document, operation.clipIds); break
      case 'move_layers': document = moveClips(document, operation.clipIds, operation.trackId, operation.startTicks); break
      case 'move_layer_placements': document = moveClipPlacements(document, operation.placements, operation.anchorId, operation.startTicks); break
      case 'move_layers_to_new_track': document = moveClipsToNewTrack(document, operation.clipIds, operation.trackId, operation.anchor); break
      case 'paste_layers': document = pasteClips(document, operation.subjects, { targetTrackId: operation.trackId, timelineStartTicks: operation.startTicks, linkIds: operation.linkIds }); break
      case 'duplicate_layer': document = duplicateClip(document, operation.clipId, { id: operation.id, targetTrackId: operation.trackId, timelineStartTicks: operation.startTicks, escapeLink: operation.escapeLink, linkId: operation.linkId }); break
      case 'create_link': document = createCompositionLink(document, operation.id, operation.memberIds, operation.reassign); break
      case 'remove_link': document = removeCompositionLink(document, operation.id); break
      case 'add_track': document = addTrack(document, operation.track, operation.index); break
      case 'delete_track': document = deleteTrack(document, operation.trackId); break
      case 'reorder_track': document = reorderTrack(document, operation.trackId, operation.index); break
      case 'add_layer': {
        document = addClip(document, operation.trackId, operation.clip)
        const admitted = findClipLocation(document,operation.clip.id).clip
        const normalized = normalizeAuthoredClipTangents(admitted)
        document = { ...document,tracks: document.tracks.map(track => track.id !== operation.trackId ? track : { ...track,clips: track.clips.map(clip => clip.id === admitted.id ? normalized : clip) } as CompositionTrack) }
        break
      }
      case 'delete_layer': document = deleteClip(document, operation.clipId); break
      case 'update_layer': {
        const location = findClipLocation(document, operation.clipId)
        if (!operation.patch || typeof operation.patch !== 'object' || Array.isArray(operation.patch)) throw new Error('Invalid layer patch')
        const lockOnly = Object.keys(operation.patch).length === 1 && typeof operation.patch.locked === 'boolean'
        if (location.track.locked && !lockOnly) throw new Error('Track is locked')
        if ('id' in operation.patch || 'kind' in operation.patch) throw new Error('Layer identity cannot be changed')
        document = { ...document, tracks: document.tracks.map((track) => track.id !== location.track.id ? track : ({ ...track, clips: track.clips.map((clip) => clip.id !== operation.clipId ? clip : { ...clip, ...operation.patch }) } as CompositionTrack)) }
        assertValidComposition(document)
        document = { ...document,tracks: document.tracks.map(track => ({ ...track,clips: track.clips.map(clip => {
          if (clip.id !== operation.clipId) return clip
          return { ...clip,
            ...('animation' in operation.patch && clip.kind !== 'audio' && clip.animation ? { animation: normalizeAuthoredTangentAnimation(clip.animation) } : {}),
            ...('colorAnimation' in operation.patch && (clip.kind === 'text' || clip.kind === 'color') && clip.colorAnimation ? { colorAnimation: normalizeColorAnimation(clip.colorAnimation) } : {}),
            ...('audioAnimation' in operation.patch && (clip.kind === 'audio' || clip.kind === 'video') && clip.audioAnimation ? { audioAnimation: normalizeAuthoredTangentAnimation(clip.audioAnimation) } : {}),
            ...('masks' in operation.patch && clip.kind === 'video' && clip.masks ? { masks: normalizedMasks(clip.masks) } : {}),
          }
        }) } as CompositionTrack)) }
        assertValidComposition(document)
        break
      }
      case 'move_layer': document = moveClip(document, operation.clipId, operation.trackId, operation.startTicks, operation.escapeLink); break
      case 'trim_layer': document = trimClip(document, operation.clipId, operation.startTicks, operation.endTicks, operation.escapeLink); break
      case 'split_layer': document = splitClip(document, operation.clipId, operation.atTicks, operation.rightClipId, operation.escapeLink); break
      case 'add_marker': document = upsertCompositionMarker(document, operation.marker); break
      case 'remove_marker': document = deleteCompositionMarker(document, operation.markerId); break
      case 'add_transition': document = upsertTransition(document, operation.trackId, operation.transition); break
      case 'remove_transition': document = deleteTransition(document, operation.trackId, operation.transitionId); break
      default: throw new Error('Unknown operation')
    }
    assertCompositionLocksPreserved(previous, document)
  }
  assertValidComposition(document)
  return document
}

function normalizedMasks(masks: readonly import('../composition/types.js').CompositionVideoMask[]): readonly import('../composition/types.js').CompositionVideoMask[] {
  return masks.map(mask => ({ ...mask,...normalizeAuthoredTangentAnimation({ x: mask.x,y: mask.y,width: mask.width,height: mask.height,...(mask.rotationDegrees ? { rotationDegrees: mask.rotationDegrees } : {}) }) }))
}
function normalizeAuthoredClipTangents(clip: CompositionClip): CompositionClip {
  return { ...clip,
    ...(clip.kind !== 'audio' && clip.animation ? { animation: normalizeAuthoredTangentAnimation(clip.animation) } : {}),
    ...((clip.kind === 'audio' || clip.kind === 'video') && clip.audioAnimation ? { audioAnimation: normalizeAuthoredTangentAnimation(clip.audioAnimation) } : {}),
    ...(clip.kind === 'video' && clip.masks ? { masks: normalizedMasks(clip.masks) } : {}),
  }
}
