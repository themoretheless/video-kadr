import {takeSourceRunCutTransaction} from '$lib/composition/sourceRunCutTransaction'
import {authState} from './auth.svelte'
import {captureNativeScalarHistoryViews,restoreNativeScalarHistoryViews,type NativeScalarHistoryViews} from '$lib/composition/nativeScalarHistoryView'
import {concatSelectionKeyTarget} from '$lib/composition/concatKeyNavigation'
import {planHistoryJump} from '$lib/agent/historyNavigation'
import {nestedAffected} from '$lib/agent/affectedEntities'
import {SvelteSet} from 'svelte/reactivity'
import {registerHistoryProvider,identifyHistoryEntry,transferHistoryEntry} from '$lib/agent/historyProvenance'
import {linkActivityHistory,linkActivityEntities,type ActivityToken} from '$lib/agent/activity.svelte'
import {agentSession,assertHistoryUnlocked,clearAgentWorkContext} from '$lib/agent/session.svelte'
import {setNativeCaptionText} from '$lib/composition/nativeCaptionText'
import {importNestedSubtitles} from '$lib/subtitles/nestedImport'
import { mergeNestedCaptions } from '$lib/composition/mergeCaptions'
import { restyleNestedCaptions, type CaptionStylePatch } from '$lib/composition/restyleCaptions'
import { nestedDurations } from '$lib/api'
import {videoRegionOrdinal} from '$lib/video-region'
import {reattachNativeConcatAudio} from '$lib/composition/concatDetachedAudio'
import {cloneGainSourceKeys} from '$lib/composition/concatGainSourceKeys'
import { moveTimeline, addTimeline, removeTimeline, setTimelineVideo, affectedTimelineIds } from '$lib/composition/timelineOrder'
import type { NativeNestedProject } from '$lib/api'

export const nestedPlayback = $state({ playing: false, epoch: 0 })

export const nestedComparison = $state({ enabled:false,referenceUs:0 })

export const nestedComposition = $state<{
  projectId: string | null
  revision: number | null
  name: string
  document: NativeNestedProject | null
  activeCompositionId: string | null
  timeUs: number
  renderedOutputs: Record<string, string>
}>({ projectId: null, revision: null, name: '', document: null, activeCompositionId: null, timeUs: 0, renderedOutputs: {} })

export function closeNestedComposition(): void {
  nestedPlayback.playing = false; nestedPlayback.epoch++
  nestedComparison.enabled=false; nestedComparison.referenceUs=0
  resetNestedHistory()
  nestedComposition.document = null
  nestedComposition.projectId = null
  nestedComposition.revision = null
  nestedComposition.activeCompositionId = null
  nestedComposition.timeUs = 0
  nestedComposition.renderedOutputs = {}
}

type ProjectContext={projectId:string|null;revision:number|null;name:string}
function projectContext():ProjectContext{return {projectId:nestedComposition.projectId,revision:nestedComposition.revision,name:nestedComposition.name}}
type Snapshot = { scalarViews?:NativeScalarHistoryViews;empty?:boolean;projectContext?:ProjectContext;document: NativeNestedProject; outputs: Record<string, string>; selection?: { id: string | null; timeUs: number } }
export const nestedHistory = $state<{ past: Snapshot[]; future: Snapshot[]; busy: boolean; error: string }>({ past: [], future: [], busy: false, error: '' })
let generation = 0
const nestedContextGeneration=$state({value:0})

function snapshot(includeSelection = false,fallback?:NativeNestedProject): Snapshot | null {
  if (!nestedComposition.document&&!fallback) return null
  const value=JSON.parse(JSON.stringify({ document: nestedComposition.document??fallback,...(!nestedComposition.document?{empty:true}:{}), outputs: nestedComposition.renderedOutputs, ...(includeSelection ? { selection: { id: nestedComposition.activeCompositionId, timeUs: nestedComposition.timeUs } } : {}) })) as Snapshot
  value.scalarViews=captureNativeScalarHistoryViews()
  return value
}
function restore(value: Snapshot): void {
  if(value.projectContext)Object.assign(nestedComposition,value.projectContext)
  if (value.selection) { nestedComposition.activeCompositionId = value.selection.id; nestedComposition.timeUs = value.selection.timeUs }
  if (!value.empty&&!value.document.compositions[nestedComposition.activeCompositionId ?? '']) nestedComposition.activeCompositionId = value.document.root
  nestedComposition.document = value.empty?null:value.document
  nestedComposition.renderedOutputs = value.outputs
  restoreNativeScalarHistoryViews(value.scalarViews)
}
export function resetNestedHistory(): void {
  clearAgentWorkContext()
  generation += 1
  nestedContextGeneration.value=generation
  nestedHistory.past = []
  nestedHistory.future = []
  nestedHistory.busy = false
  nestedHistory.error = ''
}
export function undoNestedComposition(): void {
  if (nestedHistory.busy) return
  const current = snapshot(false,nestedHistory.past.at(-1)?.document)
  const previous = nestedHistory.past.pop()
  if (!current || !previous) return
  if(previous.projectContext)current.projectContext=projectContext()
  if (previous.selection) current.selection = { id: nestedComposition.activeCompositionId, timeUs: nestedComposition.timeUs }
  nestedHistory.future.push(current)
  transferHistoryEntry(previous,nestedHistory.future.at(-1)!)
  restore(previous)
}
export function redoNestedComposition(): void {
  if (nestedHistory.busy) return
  const current = snapshot(false,nestedHistory.future.at(-1)?.document)
  const next = nestedHistory.future.pop()
  if (!current || !next) return
  if(next.projectContext)current.projectContext=projectContext()
  if (next.selection) current.selection = { id: nestedComposition.activeCompositionId, timeUs: nestedComposition.timeUs }
  nestedHistory.past.push(current)
  transferHistoryEntry(next,nestedHistory.past.at(-1)!)
  restore(next)
}
export function jumpNestedHistory(entryId:string,side:'before'|'after'='after',actor?:string):number{
 assertHistoryUnlocked(actor)
 if(nestedHistory.busy)throw new Error('Nested project is busy')
 const plan=planHistoryJump(nestedHistory.past,nestedHistory.future,entryId,side)
 for(let index=0;index<plan.steps;index++)if(plan.direction==='undo')undoNestedComposition();else redoNestedComposition()
 return plan.steps
}

async function editNestedDocument(operation: (document: NativeNestedProject) => Promise<NativeNestedProject>, invalidateOutputs: boolean, selectAfter?: string, transformOutputs?: (outputs: Record<string, string>) => Record<string, string>, timeAfter = 0, onCommit?: () => void, validateCommit?: () => void): Promise<void> {
  const sourceCut=takeSourceRunCutTransaction()
  const before = snapshot(selectAfter !== undefined)
  if (!before || nestedHistory.busy) {sourceCut?.settle(false);return}
  const currentGeneration = generation
  const identity = JSON.stringify(before.document)
  nestedHistory.busy = true
  nestedHistory.error = ''
  try {
    const document = await operation(before.document)
    if (generation !== currentGeneration || JSON.stringify(nestedComposition.document) !== identity) return
    if (JSON.stringify(document) === identity) return
    validateCommit?.()
    sourceCut?.validate()
    nestedHistory.past = [...nestedHistory.past.slice(-99), before]
    nestedHistory.future = []
    nestedComposition.document = document
    if (selectAfter !== undefined) { nestedComposition.activeCompositionId = selectAfter; nestedComposition.timeUs = timeAfter }
    // Old rendered movies cannot represent the changed group boundaries.
    if (invalidateOutputs) nestedComposition.renderedOutputs = {}
    else if (transformOutputs) nestedComposition.renderedOutputs = transformOutputs(nestedComposition.renderedOutputs)
    sourceCut?.settle(true)
    onCommit?.()
  } catch (error) {
    if (generation === currentGeneration) nestedHistory.error = error instanceof Error ? error.message : String(error)
  } finally {
    sourceCut?.settle(false)
    if (generation === currentGeneration) nestedHistory.busy = false
  }
}

export async function setNestedMediaPlaceholder(sourceId: string, placeholder: boolean): Promise<void> {
  const guard = () => assertHistoryUnlocked()
  await editNestedDocument(async document => {
    guard()
    const { setNativeProjectMediaPlaceholder } = await import('$lib/api')
    return setNativeProjectMediaPlaceholder(document, sourceId, placeholder)
  }, false, undefined, undefined, 0, undefined, guard)
}

export async function groupNestedDocument(parent: string, child: string, instanceId: string): Promise<void> {
  await editNestedDocument(async (document) => {
    const { groupNestedComposition } = await import('$lib/api')
    return groupNestedComposition(document, parent, child, instanceId)
  }, true)
}
export async function splitNestedInstance(parent: string, instanceId: string, rightId: string, timeUs: number): Promise<void> {
  await editNestedDocument(async (document) => {
    const { splitNestedComposition } = await import('$lib/api')
    return splitNestedComposition(document, parent, instanceId, rightId, timeUs)
  }, false)
}

export function navigateNestedComposition(id: string): void {
  if (!nestedComposition.document?.compositions[id]) return
  nestedComposition.activeCompositionId = id
  nestedComposition.timeUs = 0
}

/** Validate changed effects natively before recording an undoable local edit. */
export async function editNestedMetadata(compositionId: string, update: (metadata: import('$lib/composition/types').CompositionMetadata) => import('$lib/composition/types').CompositionMetadata, validateCommit?:()=>void): Promise<void> {
  await editNestedDocument(async document => {
    validateCommit?.()
    const target = document.compositions[compositionId]
    if (!target) throw new Error('Композиция не найдена')
    const metadata = update(target.content.metadata ?? {})
    if (!Number.isSafeInteger(metadata.sequenceStartFrame ?? 0)) throw new Error('Начальный кадр должен быть целым числом')
    const changed = { ...document, compositions: { ...document.compositions, [compositionId]: { ...target, content: { ...target.content, metadata } } } }
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    return changed
  }, true,undefined,undefined,0,undefined,validateCommit)
}

export async function editNestedVideoEffects(compositionId: string, trackId: string, clipId: string, update: (effects: readonly import('$lib/composition/types').WireVideoEffect[]) => readonly import('$lib/composition/types').WireVideoEffect[], validateCommit?:()=>void): Promise<void> {
  const token=authState.token,projectId=nestedComposition.projectId
  const guard=()=>{assertHistoryUnlocked();if(authState.token!==token||nestedComposition.projectId!==projectId)throw new Error('Сессия эффекта изменилась');validateCommit?.()}
  await editNestedDocument(async (document) => {
    guard()
    const target = document.compositions[compositionId]
    if (!target) throw new Error('Композиция не найдена')
    let found = false
    const tracks = target.content.tracks.map((track) => {
      if (track.id !== trackId || (track.kind !== 'video' && track.kind !== 'image' && track.kind !== 'text' && track.kind !== 'graphic')) return track
      if (track.locked) throw new Error('Дорожка заблокирована')
      const editClip = <T extends { readonly id: string; readonly effects?: readonly import('$lib/composition/types').WireVideoEffect[] }>(clip: T): T => {
        if (clip.id !== clipId) return clip
        if(track.kind==='text'&&(!('concatTitle' in clip)||!clip.concatTitle))throw new Error('Нужен титр Concat')
    if(track.kind==='graphic'&&(!('concatShape' in clip)||!clip.concatShape)&&(!('concatTreatment' in clip)||!clip.concatTreatment))throw new Error('Нужна фигура Concat или корректирующий слой')
        found = true
        return { ...clip, effects: update(clip.effects ?? []) }
      }
      if (track.kind === 'video') return { ...track, clips: track.clips.map(editClip) }
      if (track.kind === 'graphic') return { ...track, clips: track.clips.map(editClip) }
      if (track.kind === 'text') return { ...track, clips: track.clips.map(editClip) }
      return { ...track, clips: track.clips.map(editClip) }
    })
    if (!found) throw new Error('Визуальный клип не найден')
    const changed = { ...document, compositions: { ...document.compositions, [compositionId]: { ...target, content: { ...target.content, tracks } } } }
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    return changed
  }, true,undefined,undefined,0,undefined,guard)
}

export async function applyNestedColorMatch(compositionId: string, trackId: string, clipId: string, effectIndex: number, result: import('$lib/api').NativeColorMatch): Promise<void> {
  const { applyColorMatch } = await import('$lib/composition/colorMatch')
  await editNestedVideoEffects(compositionId, trackId, clipId, effects => {
    if (effects[effectIndex]?.kind !== 'lumetri') throw new Error('Эффект Lumetri не найден')
    return effects.map((effect, index) => index === effectIndex && effect.kind === 'lumetri'
      ? { ...effect, parameters: applyColorMatch(effect.parameters, result) } : effect)
  })
}

export async function importNestedProjectLut(lut:import('$lib/composition/types').WireColorLut,name:string,sourcePath:string,composition:string,track:string,clip:string,effectIndex:number,input:boolean,signal?:AbortSignal):Promise<void> {
 await editNestedDocument(async document=>{
  const {importProjectLut}=await import('$lib/api')
  return (await importProjectLut(document,lut,name,sourcePath,composition,track,clip,effectIndex,input,signal)).project
 },true)
}

export async function updateNestedProjectLut(id:string,changes:{name?:string;lut?:import('$lib/composition/types').WireColorLut;sourcePath?:string},signal?:AbortSignal):Promise<void> {
 await editNestedDocument(async document=>{
  const {updateProjectLut}=await import('$lib/api')
  return updateProjectLut(document,id,changes,signal)
 },changes.lut!==undefined)
}

export async function removeNestedProjectLut(id:string):Promise<void> {
 await editNestedDocument(async document=>{
  const {removeProjectLut}=await import('$lib/api')
  return removeProjectLut(document,id)
 },true)
}

export async function addNestedFontFile(data:string):Promise<void>{
 const guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{guard();const {editProjectFontFile}=await import('$lib/api');return editProjectFontFile(document,{kind:'add',data})},true,undefined,undefined,0,undefined,guard)
}
export async function removeNestedFontFile(index:number,resource:import('$lib/api').NativeProjectFont):Promise<void>{
 const expected={family:resource.family,data:resource.data},guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{guard();const current=document.fonts?.[index];if(!Number.isSafeInteger(index)||index<0||current?.family!==expected.family||current?.data!==expected.data)throw new Error('Выбранный файл шрифта изменился');const {editProjectFontFile}=await import('$lib/api');return editProjectFontFile(document,{kind:'remove',index})},true,undefined,undefined,0,undefined,guard)
}

export async function replaceNestedFonts(from: import('$lib/api').CompositionFontSelection,to: import('$lib/api').CompositionFontSelection):Promise<void> {
 await editNestedDocument(async document=>{
  const {replaceProjectFonts}=await import('$lib/api')
  return (await replaceProjectFonts(document,from,to)).project
 },true)
}

export async function moveNestedTimeline(id: string, index: number): Promise<void> {
  await editNestedDocument(async document => {
    const changed = moveTimeline(document, id, index)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    return changed
  }, false)
}

export async function renameNestedTimeline(id: string, name: string): Promise<void> {
  await editNestedMetadata(id, metadata => {
    const trimmed = name.trim()
    if (!trimmed || trimmed.length > 128) throw new Error('Имя таймлайна должно содержать от 1 до 128 символов')
    return { ...metadata, sequenceName: trimmed }
  })
}

export async function addNestedTimeline(id = `timeline-${crypto.randomUUID()}`): Promise<void> {
  const sourceId = nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? ''
  await editNestedDocument(async document => {
    const changed = addTimeline(document, sourceId, id)
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    return changed
  }, false, id)
}

export async function removeNestedTimeline(id: string, removePlacements = false): Promise<void> {
  if (!nestedComposition.document || nestedHistory.busy) return
  try {
    const staged = removeTimeline(nestedComposition.document, id, removePlacements)
    const active = nestedComposition.activeCompositionId ?? nestedComposition.document.root
    await editNestedDocument(async () => {
      const { evaluateNestedFrame } = await import('$lib/api')
      await evaluateNestedFrame(staged.document, 0)
      return staged.document
    }, false, active === id ? staged.neighbor : undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !staged.invalidatedIds.includes(key))))
  } catch (error) { nestedHistory.error = error instanceof Error ? error.message : String(error) }
}

export async function setNestedTimelineVideo(id: string, settings: import('$lib/composition/timelineOrder').TimelineVideoSettings): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const changed = setTimelineVideo(document, id, settings)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function moveNestedClips(moves: readonly import('$lib/api').NativeClipMove[]): Promise<void> {
  if (!moves.length) return
  const destinations = [...new SvelteSet(moves.map(move => move.toComposition))]
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { moveNativeNestedClips } = await import('$lib/api')
    const result = await moveNativeNestedClips(document, moves)
    if (!result.changed) return document
    affected = result.affectedCompositions
    return result.project
  }, false, destinations.length === 1 ? destinations[0] : undefined,
  outputs => Object.fromEntries(Object.entries(outputs).filter(([id]) => !affected.includes(id))), Math.min(...moves.map(move => move.startUs)))
}

export async function editNestedTrack(id: string, edit: import('$lib/composition/nativeTrackEdit').NativeTrackEdit): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { editNativeTrack } = await import('$lib/composition/nativeTrackEdit')
    const changed = editNativeTrack(document, id, edit)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    if (!(edit.kind === 'flag' && edit.flag === 'locked')) affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function slipNestedLayer(id:string,clipId:string,sourceDeltaTicks:number):Promise<void>{
 const guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{
  guard()
  const {slipNativeLayer}=await import('$lib/composition/nativeSlip')
  const changed=slipNativeLayer(document,id,clipId,sourceDeltaTicks)
  if(changed===document)return document
  const {evaluateNestedFrame}=await import('$lib/api')
  await evaluateNestedFrame(changed,0)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function raiseNestedLayers(id:string,ids:readonly string[],trackId:string,anchor?:{clipId:string;startTicks:number}):Promise<void>{
 const selected=[...ids],landing=anchor?{...anchor}:undefined,guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{
  guard()
  const {raiseNativeLayers}=await import('$lib/composition/nativeRaise')
  const changed=await raiseNativeLayers(document,id,selected,trackId,landing)
  const {evaluateNestedFrame}=await import('$lib/api')
  await evaluateNestedFrame(changed,0)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function rippleNestedComposition(id:string,request:{clipIds:readonly string[]}|{trackId:string;start:number;end:number}):Promise<void>{
 const guard=()=>assertHistoryUnlocked()
 const captured='clipIds' in request?{clipIds:[...request.clipIds]}:{...request}
 await editNestedDocument(async document=>{
  guard()
  const {rippleNativeComposition}=await import('$lib/composition/nativeRipple')
  const changed=await rippleNativeComposition(document,id,captured)
  const {evaluateNestedFrame}=await import('$lib/api')
  await evaluateNestedFrame(changed,0)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function removeNestedClips(id: string, ids: readonly string[]): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { removeNativeClips } = await import('$lib/composition/nativeClipDelete')
    const changed = removeNativeClips(document, id, ids)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function insertNestedMedia(id: string, source: import('$lib/composition/types').CompositionSource, request: import('$lib/composition/nativeClipInsert').NativeMediaInsert, validateCommit?: () => void): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { insertNativeMedia, insertNativeMediaAtFirstFree } = await import('$lib/composition/nativeClipInsert')
    const changed = request.automatic ? insertNativeMediaAtFirstFree(document, id, source, { ...request, newTrackId: request.newTrackId ?? `track-${crypto.randomUUID()}` }) : insertNativeMedia(document, id, source, request)
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))), 0, undefined, validateCommit)
}

export async function trimNestedClipEnds(id: string, ids: readonly string[], deltaUs: number): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { trimNativeClipsWithSourcePlans } = await import('$lib/composition/concatSourceTrim')
    const changed = await trimNativeClipsWithSourcePlans(document, id, ids, deltaUs, 'end')
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function editNestedClipLinks(id: string, ids: readonly string[], unlink = false): Promise<void> {
  await editNestedDocument(async document => {
    const { linkNativeClips, unlinkNativeClips } = await import('$lib/composition/nativeClipLinks')
    const changed = unlink ? unlinkNativeClips(document, id, ids) : linkNativeClips(document, id, ids)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    return changed
  }, false)
}

export async function splitNestedClips(id: string, ids: readonly string[], timeUs: number): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { splitNativeClipsWithSourcePlans } = await import('$lib/composition/concatSourceSplit')
    const changed = await splitNativeClipsWithSourcePlans(document, id, ids, timeUs, () => `clip-${crypto.randomUUID()}`)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function trimNestedClipStarts(id: string, ids: readonly string[], deltaUs: number): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { trimNativeClipsWithSourcePlans } = await import('$lib/composition/concatSourceTrim')
    const changed = await trimNativeClipsWithSourcePlans(document, id, ids, deltaUs, 'start')
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function replaceNestedMedia(id: string, ids: readonly string[], source: import('$lib/composition/types').CompositionSource, keepOriginal = false, sourceStartUs?: number): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { replaceNativeMedia } = await import('$lib/composition/nativeMediaReplace')
    const changed = replaceNativeMedia(document, id, ids, source, keepOriginal, sourceStartUs)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function switchNestedMediaVersion(id: string, ids: readonly string[], copy: boolean): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { setNativeMediaVersion } = await import('$lib/composition/nativeMediaReplace')
    const changed = setNativeMediaVersion(document, id, ids, copy)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}

export async function duplicateNestedClips(id: string, ids: readonly string[], startUs?: number): Promise<readonly string[]> {
  let affected: readonly string[] = [], copied: string[] = []
  await editNestedDocument(async document => {
    const { duplicateNativeClipsWithSourcePlans } = await import('$lib/composition/concatSourceCopy')
    const changed = await duplicateNativeClipsWithSourcePlans(document, id, ids, () => `clip-${crypto.randomUUID()}`, startUs)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    const old = new SvelteSet(document.compositions[id]!.content.tracks.flatMap(track => track.clips.map(clip => clip.id)))
    copied = changed.compositions[id]!.content.tracks.flatMap(track => track.clips.map(clip => clip.id)).filter(id => !old.has(id))
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
  const current = new SvelteSet(nestedComposition.document?.compositions[id]?.content.tracks.flatMap(track => track.clips.map(clip => clip.id)) ?? [])
  return copied.every(id => current.has(id)) ? copied : []
}


export async function copyNestedClips(from: string, to: string, ids: readonly string[], startUs: number, targets: Readonly<Record<string,string>>): Promise<void> {
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { prepareNativeClipCopy, finishNativeClipCopy } = await import('$lib/composition/nativeClipCopy')
    const { moveNativeNestedClips, evaluateNestedFrame } = await import('$lib/api')
    const plan = prepareNativeClipCopy(document, from, to, ids, startUs, targets, () => `copy-${crypto.randomUUID()}`)
    const result = await moveNativeNestedClips(plan.staged, plan.moves)
    const changed = finishNativeClipCopy(result, plan.temporary, document, to)
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, to)
    return changed
  }, false, to, outputs => Object.fromEntries(Object.entries(outputs).filter(([id]) => !affected.includes(id))), startUs)
}

export const nestedClipboard = $state<{ value: NativeNestedProject | null }>({ value: null })
export async function copyNestedSelectionToClipboard(id: string, ids: readonly string[]): Promise<void> {
  if (!nestedComposition.document || nestedHistory.busy) return
  const document = nestedComposition.document
  const selected = [...ids]
  try {
    const { captureNativeClipboard } = await import('$lib/composition/nativeClipboard')
    const clipboard = captureNativeClipboard(document,id,selected,()=>`clip-${crypto.randomUUID()}`)
    if (nestedComposition.document === document && !nestedHistory.busy) { nestedClipboard.value=clipboard; nestedHistory.error='' }
  } catch (error) { nestedHistory.error=error instanceof Error ? error.message : String(error) }
}
export async function pasteNestedClipboard(to: string, startUs: number, targets: Readonly<Record<string,string>> = {}): Promise<void> {
  const clipboard=nestedClipboard.value
  if (!clipboard) return
  let affected: readonly string[]=[]
  await editNestedDocument(async document=>{
    const { prepareNativeClipboardPaste }=await import('$lib/composition/nativeClipboard')
    const { finishNativeClipCopy }=await import('$lib/composition/nativeClipCopy')
    const { moveNativeNestedClips,evaluateNestedFrame }=await import('$lib/api')
    const plan=prepareNativeClipboardPaste(document,clipboard,to,startUs,targets,()=>`paste-${crypto.randomUUID()}`)
    const { pasteNativeCaptionTracks }=await import('$lib/composition/nativeCaptionClipboard')
    const media=plan.moves.length?finishNativeClipCopy(await moveNativeNestedClips(plan.staged,plan.moves),plan.temporary,plan.base,to):plan.base
    const changed=pasteNativeCaptionTracks(media,to,plan.captions,plan.captionDeltaUs,targets,()=>`caption-${crypto.randomUUID()}`)
    await evaluateNestedFrame(changed,0)
    affected=affectedTimelineIds(document,to)
    return changed
  },false,to,outputs=>Object.fromEntries(Object.entries(outputs).filter(([id])=>!affected.includes(id))),startUs)
}


export async function cutNestedSelectionToClipboard(id: string, ids: readonly string[]): Promise<void> {
  let captured: NativeNestedProject | null=null, affected: readonly string[]=[]
  const selected=[...ids], previousClipboard=nestedClipboard.value
  await editNestedDocument(async document=>{
    const { captureNativeClipboard }=await import('$lib/composition/nativeClipboard')
    const { removeNativeClips }=await import('$lib/composition/nativeClipDelete')
    const { evaluateNestedFrame }=await import('$lib/api')
    captured=captureNativeClipboard(document,id,selected,()=>`clip-${crypto.randomUUID()}`)
    const changed=removeNativeClips(document,id,selected)
    await evaluateNestedFrame(changed,0)
    affected=affectedTimelineIds(document,id)
    return changed
  },false,undefined,outputs=>Object.fromEntries(Object.entries(outputs).filter(([key])=>!affected.includes(key))),0,()=>{if(captured)nestedClipboard.value=captured},()=>{if(nestedClipboard.value!==previousClipboard)throw new Error('Буфер изменился во время проверки. Повторите вырезание.')})
}

export async function importNestedClipboardText(text: string): Promise<boolean> {
  if (!nestedComposition.document || nestedHistory.busy) return false
  const currentGeneration=generation, document=nestedComposition.document, previous=nestedClipboard.value
  try {
    const { parseNativeClipboard }=await import('$lib/composition/nativeClipboardText')
    const { evaluateNestedFrame }=await import('$lib/api')
    const candidate=parseNativeClipboard(text)
    await evaluateNestedFrame(candidate,0)
    if (generation!==currentGeneration) return false
    if (nestedComposition.document!==document || nestedHistory.busy) throw new Error('Проект изменился во время чтения буфера. Повторите действие.')
    if (nestedClipboard.value!==previous) throw new Error('Буфер изменился во время проверки. Повторите действие.')
    nestedClipboard.value=candidate;nestedHistory.error='';return true
  } catch(error) {
    if (generation===currentGeneration) nestedHistory.error=error instanceof Error?error.message:String(error)
    return false
  }
}

export async function addNestedSpeechCaptions(id:string,transcript:import('$lib/api').LocalSpeechTranscript,startUs:number):Promise<void>{
  if(!Number.isSafeInteger(startUs)||startUs<0){nestedHistory.error='Некорректное начало субтитров';return}
  let affected:readonly string[]=[]
  await editNestedDocument(async document=>{
    const {speechCaptionTrack}=await import('$lib/composition/speechCaptions')
    const {pasteNativeCaptionTracks}=await import('$lib/composition/nativeCaptionClipboard')
    const track=speechCaptionTrack(transcript)
    if(!track.clips.length)throw new Error('Речь в выбранном диапазоне не найдена')
    const changed=pasteNativeCaptionTracks(document,id,[track],startUs,{},()=>`speech-${crypto.randomUUID()}`)
    const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0)
    affected=affectedTimelineIds(document,id);return changed
  },false,undefined,outputs=>Object.fromEntries(Object.entries(outputs).filter(([key])=>!affected.includes(key))))
}

export async function addNestedAlignedSpeechCaptions(id:string,clipId:string,sourceId:string,transcript:import('$lib/api').LocalSpeechTranscript):Promise<void>{
  let affected:readonly string[]=[]
  await editNestedDocument(async document=>{
    const {alignSpeechClip,evaluateNestedFrame}=await import('$lib/api')
    const {pasteNativeCaptionTracks}=await import('$lib/composition/nativeCaptionClipboard')
    const track=await alignSpeechClip(document,id,clipId,sourceId,transcript)
    if(!track.clips.length)throw new Error('Распознанная речь не пересекает видимую часть клипа')
    const changed=pasteNativeCaptionTracks(document,id,[track],0,{},()=>`speech-${crypto.randomUUID()}`)
    await evaluateNestedFrame(changed,0);affected=affectedTimelineIds(document,id);return changed
  },false,undefined,outputs=>Object.fromEntries(Object.entries(outputs).filter(([key])=>!affected.includes(key))))
}

export async function replaceNestedCutoutMedia(id:string,ids:readonly string[],entry:import('$lib/api').CutoutAcceptedMedia):Promise<void>{
 if(!entry.cutout){const{compositionSourceFromLibraryEntry}=await import('$lib/state/composition.svelte');await replaceNestedMedia(id,ids,compositionSourceFromLibraryEntry(entry),true);return}
 const{compositionSourceFromLibraryEntry}=await import('$lib/state/composition.svelte')
 const sources={copy:compositionSourceFromLibraryEntry(entry),source:compositionSourceFromLibraryEntry(entry.cutout.source),mask:compositionSourceFromLibraryEntry(entry.cutout.mask),regions:entry.cutout.regions?.map(compositionSourceFromLibraryEntry)},snapshot=entry.cutout.snapshot
 let affected:readonly string[]=[]
 await editNestedDocument(async document=>{const{replaceNativeCutoutMedia}=await import('$lib/composition/cutoutMedia');const changed=replaceNativeCutoutMedia(document,id,ids,sources,snapshot);const{evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);affected=affectedTimelineIds(document,id);return changed},false,undefined,outputs=>Object.fromEntries(Object.entries(outputs).filter(([key])=>!affected.includes(key))))
}

export async function replaceNestedVideoCutoutMedia(id:string,ids:readonly string[],entry:import('$lib/api').SavedPersonVideo):Promise<void>{
 const{compositionSourceFromLibraryEntry}=await import('$lib/state/composition.svelte')
 if(!entry.videoCutout){await replaceNestedMedia(id,ids,compositionSourceFromLibraryEntry(entry),true);return}
 const sources={copy:compositionSourceFromLibraryEntry(entry),source:compositionSourceFromLibraryEntry(entry.videoCutout.source),mask:compositionSourceFromLibraryEntry(entry.videoCutout.mask)},snapshot=entry.videoCutout.snapshot
 let affected:readonly string[]=[]
 await editNestedDocument(async document=>{const{replaceNativeVideoCutoutMedia}=await import('$lib/composition/videoCutoutMedia');const changed=replaceNativeVideoCutoutMedia(document,id,ids,sources,snapshot);const{evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);affected=affectedTimelineIds(document,id);return changed},false,undefined,outputs=>Object.fromEntries(Object.entries(outputs).filter(([key])=>!affected.includes(key))))
}

/** One history record; tracking resources do not change current rendered pixels. */
export async function attachNestedVideoRegion(id:string,clipId:string,expected:import('$lib/composition/types').VideoMaskAttachment,entry:import('$lib/api').SavedSamVideoRegion):Promise<void>{
 const {compositionSourceFromLibraryEntry}=await import('$lib/state/composition.svelte')
 let committed=false
 await editNestedDocument(async document=>{
  const {attachNativeVideoRegion}=await import('$lib/composition/videoRegionAttach')
  const changed=attachNativeVideoRegion(document,id,clipId,expected,compositionSourceFromLibraryEntry(entry.source),compositionSourceFromLibraryEntry(entry.mask),entry.snapshot)
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0)
  return changed
 },false,undefined,undefined,0,()=>{committed=true})
 if(!committed)throw new Error(nestedHistory.error||'Проект изменился; повторно откройте видеомаску')
}

/** Open an imported document as an unsaved project, retaining the previous editor in undo. */
export async function adoptImportedConcatProject(project:NativeNestedProject,name:string,validateCommit?:()=>void):Promise<boolean>{
 if(nestedHistory.busy)return false
 const candidate=JSON.parse(JSON.stringify(project)) as NativeNestedProject
 const before=snapshot(true,candidate)!
 before.projectContext=projectContext()
 const identity=JSON.stringify(nestedComposition.document),currentGeneration=generation
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(candidate,0)
  if(generation!==currentGeneration||JSON.stringify(nestedComposition.document)!==identity)return false
  validateCommit?.()
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  nestedComposition.document=candidate;nestedComposition.projectId=null;nestedComposition.revision=null;nestedComposition.name=name.trim()||'Импорт Concat';nestedComposition.activeCompositionId=candidate.root;nestedComposition.timeUs=0;nestedComposition.renderedOutputs={}
  return true
 }catch(error){if(generation===currentGeneration)nestedHistory.error=error instanceof Error?error.message:String(error);return false}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

export async function editNestedEmbeddedAudio(compositionId:string,trackId:string,clipId:string,update:(clip:import('$lib/composition/types').WireVideoClip)=>Partial<Pick<import('$lib/composition/types').WireVideoClip,'dspEffects'|'independentAudio'|'sourceAudioEnabled'|'audioGain'|'audioPan'|'audioFadeInTicks'|'audioFadeOutTicks'>>,validateCommit?:()=>void):Promise<void>{
 await editNestedDocument(async document=>{
  const target=document.compositions[compositionId];if(!target)throw new Error('Композиция не найдена')
  let found=false
  const tracks=target.content.tracks.map(track=>{
   if(track.id!==trackId||track.kind!=='video')return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   return {...track,clips:track.clips.map(clip=>{if(clip.id!==clipId)return clip;found=true;return {...clip,...update(clip)}})}
  })
  if(!found)throw new Error('Видеоклип не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...target,content:{...target.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed
 },true,undefined,undefined,0,undefined,validateCommit)
}

export async function editNestedSourceGainKeys(compositionId:string,trackId:string,clipId:string,keys:readonly import('$lib/composition/types').WireConcatParamKey[],validateCommit?:()=>void):Promise<void>{
 const copied=cloneGainSourceKeys(keys)
 await editNestedDocument(async document=>{
  const node=document.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
  let found=false
  const tracks=node.content.tracks.map(track=>{
   if(track.id!==trackId||(track.kind!=='video'&&track.kind!=='audio'))return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   function change<T extends import('$lib/composition/types').WireVideoClip|import('$lib/composition/types').WireAudioClip>(clip:T):T {
    if(clip.id!==clipId)return clip
    found=true;const gain='audioGain'in clip?clip.audioGain:clip.gain
    if(copied.length&&gain?.mode==='keyframes')throw new Error('Сначала отключите анимацию громкости по времени')
    return {...clip,concatGainKeys:copied,...(track.kind==='video'?{independentAudio:true}:{})}
   }
   return track.kind==='video'?{...track,clips:track.clips.map(change)}:{...track,clips:track.clips.map(change)}
  })
  if(!found)throw new Error('Звуковой клип не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed
 },true,undefined,undefined,0,undefined,validateCommit)
}

export async function reattachNestedConcatAudio(compositionId:string,clipId:string):Promise<void>{
 const before=nestedComposition.document,active=nestedComposition.activeCompositionId
 await editNestedDocument(async document=>{const changed=reattachNativeConcatAudio(document,compositionId,clipId);const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed},true,undefined,undefined,0,undefined,()=>{if(nestedComposition.document!==before||nestedComposition.activeCompositionId!==active)throw new Error('Композиция изменилась')})
}


export async function editNestedAudioStream(compositionId:string,trackId:string,clipId:string,stream:number|undefined,validateCommit?:()=>void):Promise<void>{
 await editNestedDocument(async document=>{
  if(stream!==undefined&&(!Number.isInteger(stream)||stream<0||stream>0xffffffff))throw new Error('Неверный индекс аудиопотока')
  const node=document.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
  let found=false
  const tracks=node.content.tracks.map(track=>{
   if(track.id!==trackId||(track.kind!=='audio'&&track.kind!=='video'))return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   function change<T extends import('$lib/composition/types').WireVideoClip|import('$lib/composition/types').WireAudioClip>(clip:T):T{
    if(clip.id!==clipId)return clip
    found=true
    const changed={...clip,audioStream:stream};if(stream===undefined)delete changed.audioStream
    return changed
   }
   return track.kind==='video'?{...track,clips:track.clips.map(change)}:{...track,clips:track.clips.map(change)}
  })
  if(!found)throw new Error('Аудио- или видеоклип не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed
 },true,undefined,undefined,0,undefined,validateCommit)
}

export async function reactivateNestedSourceEffect(compositionId:string,trackId:string,clipId:string,index:number,validateCommit?:()=>void):Promise<void>{
  const guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
    guard()
  const node=document.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
  const track=node.content.tracks.find(track=>track.id===trackId)
  if(!track||(track.kind!=='video'&&track.kind!=='image'&&track.kind!=='text'&&track.kind!=='graphic'))throw new Error('Визуальная дорожка не найдена')
  if(track.locked)throw new Error('Дорожка заблокирована')
  const clip=track.clips.find(clip=>clip.id===clipId),effect=clip?.effects?.[index]
  if(track.kind==='text'&&(!clip||!('concatTitle' in clip)||!clip.concatTitle))throw new Error('Нужен титр Concat')
  if(track.kind==='graphic'&&(!clip||(!('concatShape' in clip)||!clip.concatShape)&&(!('concatTreatment' in clip)||!clip.concatTreatment)))throw new Error('Нужна фигура Concat или корректирующий слой')
  if(!Number.isInteger(index)||!effect)throw new Error('Визуальный эффект не найден')
  const {upgradeConcatEffectLinks,evaluateNestedFrame}=await import('$lib/api')
  let native:import('$lib/composition/types').WireVideoEffect
  if(effect.kind==='concat_bypass'){
   const result=await upgradeConcatEffectLinks([{...effect.link,enabled:true}],undefined,true)
   const converted=result.nativeEffects?.[0]
   if(result.links.length!==1||!converted||converted.kind==='concat_bypass'||converted.kind==='native_bypass')throw new Error(`Эффект ${effect.link.id} пока нельзя включить: нативная реализация отсутствует`)
   native=converted
  }else if(effect.kind==='native_bypass')native=effect.effect
  else native={kind:'native_bypass',effect}
  const replace=<T extends {readonly id:string;readonly effects?:readonly import('$lib/composition/types').WireVideoEffect[]}>(candidate:T):T=>candidate.id===clipId?{...candidate,effects:candidate.effects!.map((effect,i)=>i===index?native:effect)}:candidate
  const tracks=node.content.tracks.map(candidate=>candidate.id===trackId&&(candidate.kind==='video'||candidate.kind==='image'||candidate.kind==='text'||candidate.kind==='graphic')?candidate.kind==='video'?{...candidate,clips:candidate.clips.map(replace)}:candidate.kind==='graphic'?{...candidate,clips:candidate.clips.map(replace)}:candidate.kind==='text'?{...candidate,clips:candidate.clips.map(replace)}:{...candidate,clips:candidate.clips.map(replace)}:candidate)
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
  await evaluateNestedFrame(changed,0)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function editNestedConcatClipKeys(compositionId:string,trackId:string,clipId:string,property:string,keys:readonly import('$lib/composition/types').WireConcatParamKey[],validateCommit?:()=>void):Promise<void>{
  const guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
    guard()
  const node=document.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
  const bounds:Record<string,readonly[number,number]>={scale:[0.05,8],offsetX:[-3,3],offsetY:[-3,3],opacity:[0,1],rotation:[-Infinity,Infinity]}
  const range=bounds[property];if(!range)throw new Error('Неизвестное свойство Concat')
  if(keys.length>4096||keys.some(key=>!Number.isFinite(key.at)||key.at<0||key.at>1||!Number.isFinite(key.value)||!key.derived&&(key.value<range[0]||key.value>range[1])))throw new Error('Недопустимые ключи свойства Concat')
  const {initializeConcatVisualAnimation}=await import('$lib/composition/concatTitleAnimation')
  let found=false
  const tracks=node.content.tracks.map(track=>{
   if(track.id!==trackId||(track.kind!=='video'&&track.kind!=='image'&&track.kind!=='text'&&track.kind!=='graphic'))return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   const change=<T extends {readonly id:string;readonly transform:import('$lib/composition/types').WireTransform}>(clip:T):T=>{
    if(clip.id!==clipId)return clip
    if(track.kind==='text'&&(!('concatTitle' in clip)||!clip.concatTitle))throw new Error('Нужен титр Concat')
    if(track.kind==='graphic'&&(!('concatShape' in clip)||!clip.concatShape))throw new Error('Нужна фигура Concat')
    const title=track.kind==='text'?track.clips.find(candidate=>candidate.id===clip.id):track.kind==='graphic'?track.clips.find(candidate=>candidate.id===clip.id):undefined
    const animation=clip.transform.concatAnimation??(title?initializeConcatVisualAnimation(title,node.content.canvas):undefined);if(!animation)throw new Error('Исходная анимация Concat не найдена')
    found=true
    const runs={...animation.keys,[property]:keys.map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))}
    if(Object.values(runs).reduce((n,run)=>n+(run?.length??0),0)>8192)throw new Error('Слишком много ключей клипа Concat')
    return {...clip,transform:{...clip.transform,concatAnimation:{...animation,keys:runs}}}
   }
   return track.kind==='video'?{...track,clips:track.clips.map(change)}:track.kind==='graphic'?{...track,clips:track.clips.map(change)}:track.kind==='text'?{...track,clips:track.clips.map(change)}:{...track,clips:track.clips.map(change)}
  })
  if(!found)throw new Error('Визуальный клип не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function editNestedConcatPreparation(compositionId:string,trackId:string,clipId:string,edit:import('$lib/composition/concatPreparation').ConcatPreparationEdit,validateCommit?:()=>void):Promise<void>{
  const guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
    guard()
  const node=document.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
  const {editConcatPreparation}=await import('$lib/composition/concatPreparation');let found=false
  const tracks=node.content.tracks.map(track=>{
   if(track.id!==trackId||(track.kind!=='video'&&track.kind!=='image'&&track.kind!=='text'&&track.kind!=='graphic'))return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   const change=<T extends {readonly id:string;readonly transform:import('$lib/composition/types').WireTransform}>(clip:T):T=>{
    if(clip.id!==clipId)return clip
    if(track.kind==='text'&&(!('concatTitle' in clip)||!clip.concatTitle))throw new Error('Нужен титр Concat')
    if(track.kind==='graphic'&&(!('concatShape' in clip)||!clip.concatShape))throw new Error('Нужна фигура Concat')
    const preparation=clip.transform.concatPreparation??(track.kind==='text'||track.kind==='graphic'?{crop:[0,0,0,0] as const,canvas:[node.content.canvas.width,node.content.canvas.height] as const}:undefined);if(!preparation)throw new Error('Исходная геометрия Concat не найдена')
    found=true;return {...clip,transform:{...clip.transform,concatPreparation:editConcatPreparation(preparation,edit)}}
   }
   return track.kind==='video'?{...track,clips:track.clips.map(change)}:track.kind==='graphic'?{...track,clips:track.clips.map(change)}:track.kind==='text'?{...track,clips:track.clips.map(change)}:{...track,clips:track.clips.map(change)}
  })
  if(!found)throw new Error('Визуальный клип не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...node,content:{...node.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0);return changed
 },true,undefined,undefined,0,undefined,guard)
}

function canonicalCutoutDocument(value:unknown):string{
 if(Array.isArray(value))return `[${value.map(canonicalCutoutDocument).join(',')}]`
 if(value&&typeof value==='object')return `{${Object.entries(value).filter(([,v])=>v!==undefined).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${JSON.stringify(k)}:${canonicalCutoutDocument(v)}`).join(',')}}`
 return JSON.stringify(value)??'null'
}
/** Server mutation requires the exact saved local document, then commits once. */
export async function analyseNestedImageCutout(token:string,compositionId:string,clipId:string,subject:import('$lib/api').ConcatCutoutSubject,feather:number,signal?:AbortSignal):Promise<boolean>{
 const projectId=nestedComposition.projectId,revision=nestedComposition.revision,before=snapshot(true)
 if(nestedHistory.busy||!projectId||revision===null||!before)return false
 const currentGeneration=generation,identity=JSON.stringify(before.document)
 nestedHistory.busy=true;nestedHistory.error=''
 const current=()=>!signal?.aborted&&generation===currentGeneration&&nestedComposition.projectId===projectId&&nestedComposition.revision===revision&&JSON.stringify(nestedComposition.document)===identity
 try{
  const api=await import('$lib/api'),saved=await api.getNestedCompositionProject(projectId,token)
  if(!current())return false
  if(saved.revision!==revision||canonicalCutoutDocument(saved.document)!==canonicalCutoutDocument(before.document))throw new Error('Сначала сохраните изменения проекта и обновите его при конфликте версий')
  const correction=before.document.compositions[compositionId]?.content.metadata?.mediaVersions?.[clipId]?.cutout?.correction
  const updated=correction?await api.analyseNestedCutout(projectId,compositionId,clipId,revision,subject,feather,token,signal,correction):await api.analyseNestedCutout(projectId,compositionId,clipId,revision,subject,feather,token,signal)
  if(!current())return false
  if(updated.id!==projectId||updated.revision!==revision+1||!updated.document.compositions[compositionId])throw new Error('Сервер вернул другой проект или ревизию')
  await api.evaluateNestedFrame(updated.document,0,signal)
  if(!current())return false
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  nestedComposition.document=updated.document;nestedComposition.revision=updated.revision;nestedComposition.name=updated.name;nestedComposition.renderedOutputs={}
  return true
 }catch(error){if(generation===currentGeneration&&!signal?.aborted)nestedHistory.error=error instanceof Error?error.message:String(error);return false}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

async function savedVideoCutoutRequest<T>(token:string,compositionId:string,signal:AbortSignal|undefined,operation:(projectId:string,revision:number)=>Promise<T>):Promise<T|null>{
 const projectId=nestedComposition.projectId,revision=nestedComposition.revision,before=snapshot(true)
 if(nestedHistory.busy||!projectId||revision===null||!before||signal?.aborted)return null
 const currentGeneration=generation,identity=JSON.stringify(before.document),timeUs=nestedComposition.timeUs
 const current=()=>!signal?.aborted&&generation===currentGeneration&&nestedComposition.projectId===projectId&&nestedComposition.revision===revision&&(nestedComposition.activeCompositionId??nestedComposition.document?.root)===compositionId&&nestedComposition.timeUs===timeUs&&JSON.stringify(nestedComposition.document)===identity
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  const {getNestedCompositionProject}=await import('$lib/api'),saved=await getNestedCompositionProject(projectId,token)
  if(!current())return null
  if(saved.revision!==revision||canonicalCutoutDocument(saved.document)!==canonicalCutoutDocument(before.document))throw new Error('Сначала сохраните изменения проекта и обновите его при конфликте версий')
  const result=await operation(projectId,revision)
  return current()?result:null
 }catch(error){if(generation===currentGeneration&&!signal?.aborted)nestedHistory.error=error instanceof Error?error.message:String(error);return null}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}
export async function analyseNestedSavedVideoCutout(token:string,compositionId:string,clipId:string,signal?:AbortSignal,subject:import('$lib/api').ConcatCutoutSubject='person'):Promise<import('$lib/api').ConcatVideoAnalysis|null>{
 return savedVideoCutoutRequest(token,compositionId,signal,async(project,revision)=>{const api=await import('$lib/api');return api.analyseNestedVideoCutout(project,compositionId,clipId,revision,token,signal,subject)})
}
export async function previewNestedSavedVideoCutout(token:string,compositionId:string,clipId:string,timeUs:number,signal?:AbortSignal,feather?:number,correction?:import('$lib/api').MaskCorrection,sourceSpace?:boolean,sourceOriginal?:boolean,regionTracks?:readonly import('$lib/composition/types').ConcatVideoRegionTrack[]):Promise<import('$lib/api').NativeNestedPreviewFrame|null>{
 return savedVideoCutoutRequest(token,compositionId,signal,async(project,revision)=>{const api=await import('$lib/api');return regionTracks!==undefined?api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal,feather,correction,sourceSpace,sourceOriginal,regionTracks):sourceOriginal!==undefined?api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal,feather,correction,sourceSpace,sourceOriginal):sourceSpace!==undefined?api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal,feather,correction,sourceSpace):correction!==undefined?api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal,feather,correction):feather===undefined?api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal):api.previewNestedVideoCutout(project,compositionId,clipId,revision,timeUs,token,signal,feather)})
}

/** Save one clip's cutout selection; render providers resolve the verified mask data. */
export async function setNestedSavedVideoCutout(token:string,compositionId:string,clipId:string,enabled:boolean,signal?:AbortSignal,subject?:import('$lib/api').ConcatCutoutSubject,expectedModelHash?:string,feather?:number,correction?:import('$lib/api').MaskCorrection,regionTracks?:readonly import('$lib/composition/types').ConcatVideoRegionTrack[]):Promise<boolean>{
 const projectId=nestedComposition.projectId,revision=nestedComposition.revision,before=snapshot(true)
 if(nestedHistory.busy||!projectId||revision===null||!before||signal?.aborted)return false
 const currentGeneration=generation,identity=JSON.stringify(before.document)
 const current=()=>!signal?.aborted&&generation===currentGeneration&&nestedComposition.projectId===projectId&&nestedComposition.revision===revision&&(nestedComposition.activeCompositionId??nestedComposition.document?.root)===compositionId&&JSON.stringify(nestedComposition.document)===identity
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  const api=await import('$lib/api'),saved=await api.getNestedCompositionProject(projectId,token)
  if(!current())return false
  if(saved.revision!==revision||canonicalCutoutDocument(saved.document)!==canonicalCutoutDocument(before.document))throw new Error('Сначала сохраните изменения проекта и обновите его при конфликте версий')
  const updated=await (regionTracks!==undefined?api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal,subject,expectedModelHash,feather,correction,regionTracks):correction!==undefined?api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal,subject,expectedModelHash,feather,correction):feather!==undefined?api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal,subject,expectedModelHash,feather):expectedModelHash!==undefined?api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal,subject,expectedModelHash):subject===undefined?api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal):api.setNestedVideoCutoutBinding(projectId,compositionId,clipId,revision,enabled,token,signal,subject))
  if(!current())return false
  if(updated.id!==projectId||!updated.document.compositions[compositionId]||(updated.revision!==revision&&updated.revision!==revision+1))throw new Error('Сервер вернул другой проект или ревизию')
  const target=updated.document.compositions[compositionId]!.content.tracks.flatMap(track=>track.kind==='video'?track.clips:[]).find(clip=>clip.id===clipId)
  if(!target||Boolean(target.concatCutout)!==enabled||(enabled&&subject!==undefined&&target.concatCutout?.subject!==subject)||(enabled&&expectedModelHash!==undefined&&target.concatCutout?.modelHash!==expectedModelHash)||(enabled&&feather!==undefined&&(target.concatCutout?.feather??0)!==feather))throw new Error('Сервер не подтвердил настройку вырезания клипа')
  const correctionKey=(value:import('$lib/api').MaskCorrection|undefined)=>value&&JSON.stringify([value.custom,value.feather,value.strokes.map(s=>[s.tool,s.size,s.points,s.at??null,s.region??null])])
  if(enabled&&correction!==undefined&&correctionKey(target.concatCutout?.correction)!==correctionKey(correction))throw new Error('Сервер не подтвердил штрихи видеомаски')
  const tracksKey=(tracks:readonly import('$lib/composition/types').ConcatVideoRegionTrack[])=>canonicalCutoutDocument([...tracks].sort((a,b)=>a.stroke-b.stroke))
  if(enabled&&regionTracks!==undefined&&tracksKey(target.concatCutout?.regionTracks??[])!==tracksKey(regionTracks))throw new Error('Сервер не подтвердил временные регионы видеомаски')
  if(updated.revision===revision){if(canonicalCutoutDocument(updated.document)!==canonicalCutoutDocument(before.document))throw new Error('Документ изменился без новой ревизии');return true}
  await api.evaluateNestedFrame(updated.document,nestedComposition.timeUs,signal)
  if(!current())return false
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  nestedComposition.document=updated.document;nestedComposition.revision=updated.revision;nestedComposition.name=updated.name;nestedComposition.renderedOutputs={}
  return true
 }catch(error){if(generation===currentGeneration&&!signal?.aborted)nestedHistory.error=error instanceof Error?error.message:String(error);return false}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

/** Start durable tracking from a saved cursor; completion attachment is a separate revisioned action. */
export async function startNestedSavedVideoSmartRegion(token:string,compositionId:string,clipId:string,points:readonly [number,number][],signal?:AbortSignal,operationId?:string):Promise<import('$lib/api').NestedVideoSmartRegionJob|null>{
 const time=nestedComposition.timeUs,clip=nestedComposition.document?.compositions[compositionId]?.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===clipId)
 if(!clip)return null
 return savedVideoCutoutRequest(token,compositionId,signal,async(project,revision)=>{
  const api=await import('$lib/api'),result=await api.startNestedVideoSmartRegion(project,compositionId,clipId,revision,time,points,token,signal,...(operationId?[operationId] as [string]:[]))
  if(result.sourceId!==clip.sourceId||result.expectedRevision!==revision||typeof result.jobId!=='string'||!result.jobId||!Number.isSafeInteger(result.sourceUs)||result.sourceUs<0||!Number.isSafeInteger(result.frames)||result.frames<1||result.frames>360000||!Number.isSafeInteger(result.seedFrame)||result.seedFrame<0||result.seedFrame>=result.frames||!result.frameRate||![result.frameRate.numerator,result.frameRate.denominator].every(n=>Number.isInteger(n)&&n>0&&n<=4294967295)||videoRegionOrdinal(result,result.sourceUs)!==result.seedFrame)throw new Error('Сервер вернул другую привязку умной видеокисти')
  return result
 })
}

/** Attach a completed SAM result to the unchanged saved cursor as one undoable edit. */
export async function attachNestedSavedVideoSmartRegion(token:string,compositionId:string,clipId:string,timeUs:number,started:import('$lib/api').NestedVideoSmartRegionJob,points:readonly [number,number][],tool:'smartBrush'|'smartEraser',size:number,signal?:AbortSignal):Promise<boolean>{
 const projectId=nestedComposition.projectId,revision=nestedComposition.revision,before=snapshot(true)
 const original=before?.document.compositions[compositionId]?.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===clipId)?.concatCutout
 if(nestedHistory.busy||!projectId||revision===null||!before||!original||signal?.aborted||nestedComposition.timeUs!==timeUs||started.expectedRevision!==revision)return false
 const currentGeneration=generation,identity=JSON.stringify(before.document)
 const current=()=>!signal?.aborted&&generation===currentGeneration&&nestedComposition.projectId===projectId&&nestedComposition.revision===revision&&nestedComposition.timeUs===timeUs&&(nestedComposition.activeCompositionId??nestedComposition.document?.root)===compositionId&&JSON.stringify(nestedComposition.document)===identity
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  const api=await import('$lib/api'),saved=await api.getNestedCompositionProject(projectId,token)
  if(!current())return false
  if(saved.revision!==revision||canonicalCutoutDocument(saved.document)!==canonicalCutoutDocument(before.document))throw new Error('Сначала сохраните изменения проекта и обновите его при конфликте версий')
  const updated=await api.attachNestedVideoSmartRegion(projectId,compositionId,clipId,revision,timeUs,started.jobId,tool,size,token,signal)
  if(!current())return false
  if(updated.id!==projectId||updated.revision!==revision+1)throw new Error('Сервер вернул другой проект или ревизию')
  const node=updated.document.compositions[compositionId],clip=node?.content.tracks.flatMap(t=>t.kind==='video'?t.clips:[]).find(c=>c.id===clipId),binding=clip?.concatCutout
  if(!binding||clip.sourceId!==started.sourceId||(['subject','sourceHash','modelHash','runtimeHash'] as const).some(key=>binding[key]!==original[key]))throw new Error('Сервер изменил исходник умной видеокисти')
  const previous=original.correction??{custom:false,strokes:[],feather:original.feather??0},correction=binding.correction,index=previous.strokes.length,stroke=correction?.strokes[index]
  const strokeKey=(s:import('$lib/api').MaskStroke)=>canonicalCutoutDocument([s.tool,s.size,s.points,s.at??null,s.region??null])
  if(!correction?.custom||correction.feather!==previous.feather||correction.strokes.length!==index+1||correction.strokes.slice(0,index).some((s,i)=>strokeKey(s)!==strokeKey(previous.strokes[i]!))||!stroke||stroke.tool!==tool||stroke.size!==size||stroke.region!==undefined||typeof stroke.at!=='number'||Math.round(stroke.at*1_000_000)!==started.sourceUs||canonicalCutoutDocument(stroke.points)!==canonicalCutoutDocument(points))throw new Error('Сервер не подтвердил штрих умной видеокисти')
  const tracks=binding.regionTracks??[],oldTracks=original.regionTracks??[],added=tracks.filter(t=>t.stroke===index),remaining=tracks.filter(t=>t.stroke!==index)
  const tracksKey=(value:typeof tracks)=>canonicalCutoutDocument([...value].sort((a,b)=>a.stroke-b.stroke))
  if(added.length!==1||tracks.length!==oldTracks.length+1||tracksKey(remaining)!==tracksKey(oldTracks))throw new Error('Сервер не подтвердил временные регионы')
  const region=added[0]!.snapshot;const regionValidation:typeof import('$lib/video-region')=await import('$lib/video-region');regionValidation.validateVideoRegion(region)
  if(region.sourceId!==started.sourceId||region.sourceHash!==original.sourceHash||region.seedFrame!==started.seedFrame||region.frames!==started.frames||canonicalCutoutDocument(region.frameTimesUs)!==canonicalCutoutDocument(started.frameTimesUs)||BigInt(region.frameRate.numerator)*BigInt(started.frameRate.denominator)!==BigInt(started.frameRate.numerator)*BigInt(region.frameRate.denominator)||canonicalCutoutDocument(region.points)!==canonicalCutoutDocument(points))throw new Error('Сервер вернул другую область SAM')
  const source=node!.content.sources[region.maskSourceId]
  if(source?.kind!=='video'||source.hasAudio||source.width!==256||source.height!==256)throw new Error('Сервер не сохранил видео области SAM')
  await api.evaluateNestedFrame(updated.document,timeUs,signal)
  if(!current())return false
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  nestedComposition.document=updated.document;nestedComposition.revision=updated.revision;nestedComposition.name=updated.name;nestedComposition.renderedOutputs={}
  return true
 }catch(error){if(generation===currentGeneration&&!signal?.aborted)nestedHistory.error=error instanceof Error?error.message:String(error);return false}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

/** Reconcile a lost attachment response without posting another mutation. */
export async function reconcileNestedSavedVideoSmartRegion(token:string,intent:import('$lib/composition/pendingVideoSmart').PendingVideoSmartStroke,job:import('$lib/types').Job,signal?:AbortSignal):Promise<'applied'|'unapplied'|'blocked'>{
 const before=snapshot(true),revision=nestedComposition.revision,name=nestedComposition.name
 if(!before||!intent.base||nestedHistory.busy||nestedComposition.projectId!==intent.projectId||revision===null||signal?.aborted)return 'blocked'
 const currentGeneration=generation,identity=JSON.stringify(before.document),time=nestedComposition.timeUs
 const current=()=>!signal?.aborted&&generation===currentGeneration&&nestedComposition.projectId===intent.projectId&&nestedComposition.revision===revision&&nestedComposition.name===name&&nestedComposition.timeUs===time&&JSON.stringify(nestedComposition.document)===identity
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  const api=await import('$lib/api'),receipt=await import('$lib/composition/smartVideoReceipt'),saved=await api.getNestedCompositionProject(intent.projectId,token)
  if(!current())return 'blocked'
  if(saved.revision===intent.started.expectedRevision){if(await receipt.smartDocumentHash(saved.document)!==intent.base.documentHash)throw new Error('Исходный документ правки SAM изменился');return 'unapplied'}
  if(!await receipt.verifiedSmartVideoAppend(intent,saved,job))throw new Error('Не удалось подтвердить сохранённую правку SAM; повторное присоединение остановлено')
  if(!current())return 'blocked'
  if(saved.revision===revision&&canonicalCutoutDocument(saved.document)===canonicalCutoutDocument(before.document))return 'applied'
  if(revision!==intent.started.expectedRevision||name!==saved.name||await receipt.smartDocumentHash(before.document)!==intent.base.documentHash)throw new Error('Локальные изменения нельзя заменить результатом SAM')
  await api.evaluateNestedFrame(saved.document,time,signal)
  if(!current())return 'blocked'
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  nestedComposition.document=saved.document;nestedComposition.revision=saved.revision;nestedComposition.name=saved.name;nestedComposition.renderedOutputs={}
  return 'applied'
 }catch(error){if(generation===currentGeneration&&!signal?.aborted)nestedHistory.error=error instanceof Error?error.message:String(error);return 'blocked'}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

export async function restyleNestedProjectCaptions(patch: CaptionStylePatch, ids?: readonly string[] | null): Promise<void> {
  if (!nestedComposition.document || nestedHistory.busy) throw new Error('Проект недоступен для изменения')
  let expected = ''
  await editNestedDocument(async project => {
    const result = restyleNestedCaptions(project,patch,ids)
    expected = JSON.stringify(result.document)
    await nestedDurations(result.document)
    return result.document
  },true)
  if (nestedHistory.error) throw new Error(nestedHistory.error)
  if (!expected || JSON.stringify(nestedComposition.document) !== expected) throw new Error('Проект изменился во время проверки оформления')
}

export async function mergeNestedCaptionClips(compositionId: string, ids: readonly string[]): Promise<void> {
  if (!nestedComposition.document || nestedHistory.busy) return
  const startedGeneration = generation
  let expected = ''
  await editNestedDocument(async project => {
    const result = mergeNestedCaptions(project,compositionId,ids)
    expected = JSON.stringify(result.document)
    await nestedDurations(result.document)
    return result.document
  },true)
  if (generation !== startedGeneration) return
  if (!nestedHistory.error && expected && JSON.stringify(nestedComposition.document) !== expected) nestedHistory.error = 'Проект изменился во время объединения субтитров'
}

export async function importNestedCaptionFile(compositionId:string, serialized:string, offsetUs=0):Promise<boolean> {
  if(!nestedComposition.document||nestedHistory.busy)return false
  const startedGeneration=generation
  let expected=''
  await editNestedDocument(async project=>{
    const result=importNestedSubtitles(project,compositionId,serialized,prefix=>`${prefix}-${crypto.randomUUID()}`,offsetUs)
    expected=JSON.stringify(result.document)
    await nestedDurations(result.document)
    return result.document
  },true)
  if(generation!==startedGeneration)return false
  if(!nestedHistory.error&&expected&&JSON.stringify(nestedComposition.document)!==expected)nestedHistory.error='Проект изменился во время импорта субтитров'
  return !nestedHistory.error&&!!expected&&JSON.stringify(nestedComposition.document)===expected
}

/** Session generation belongs to the document context, not its saved DB revision. */
export function nestedAgentGeneration(): number { return nestedContextGeneration.value }
export async function applyNestedAgentDocument(expected: string, document: NativeNestedProject, admission: () => void, selection?: {id:string;timeUs:number}, activity?:ActivityToken): Promise<void> {
  if (!nestedComposition.document || nestedHistory.busy) throw new Error('Nested project is busy or closed')
  if (JSON.stringify(nestedComposition.document) !== expected) throw new Error('Project changed; read get_project again')
  let committed = false
  let selectedTime=0
  await editNestedDocument(async current => {
    if (JSON.stringify(current) !== expected) throw new Error('Project changed; read get_project again')
    const durations=await nestedDurations(document)
    if(selection)selectedTime=Math.min(Number.isSafeInteger(selection.timeUs)&&selection.timeUs>=0?selection.timeUs:0,Math.max(0,(durations[selection.id]??0)-1))
    return document
  },true,selection?.id,undefined,0,()=>{committed=true;const entry=nestedHistory.past.at(-1)!;linkActivityHistory(activity,identifyHistoryEntry(entry));if(activity)linkActivityEntities(activity,nestedAffected(entry.document,document));if(selection)nestedComposition.timeUs=selectedTime},admission)
  if (nestedHistory.error) throw new Error(nestedHistory.error)
  if (!committed) {
    admission()
    if (JSON.stringify(document) !== expected) throw new Error('Project changed; edit was not committed')
  }
}

export async function updateNestedCaptionText(compositionId:string,clipId:string,text:string,expectedText:string):Promise<boolean> {
  if(!nestedComposition.document||nestedHistory.busy)return false
  const startedGeneration=generation
  let expected=''
  await editNestedDocument(async project=>{
    const document=setNativeCaptionText(project,compositionId,clipId,text,expectedText)
    expected=JSON.stringify(document)
    await nestedDurations(document)
    return document
  },true)
  if(generation!==startedGeneration)return false
  if(!nestedHistory.error&&expected&&JSON.stringify(nestedComposition.document)!==expected)nestedHistory.error='Проект изменился во время сохранения текста'
  return !nestedHistory.error&&!!expected&&JSON.stringify(nestedComposition.document)===expected
}

registerHistoryProvider('nested',()=>nestedHistory)

/** Edit retained source title typography through native validation and shared history. */
export async function editNestedConcatTitle(compositionId:string,trackId:string,clipId:string,patch:Partial<import('$lib/composition/types').WireConcatTitleStyle>,validateCommit?:()=>void,textEdit?:{value:string;expected:string}):Promise<void>{
 const detached=JSON.parse(JSON.stringify(patch)) as typeof patch
 const guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard()
  const target=document.compositions[compositionId]
  if(!target)throw new Error('Композиция не найдена')
  let found=false
  const tracks=target.content.tracks.map(track=>{
   if(track.id!==trackId||track.kind!=='text')return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   return {...track,clips:track.clips.map(clip=>{
    if(clip.id!==clipId)return clip
    if(!clip.concatTitle)throw new Error('Выбранный клип не является титром Concat')
    if(textEdit&&clip.text!==textEdit.expected)throw new Error('Текст титра изменился после начала редактирования')
    found=true;return {...clip,...(textEdit?{text:textEdit.value}:{}),concatTitle:{...clip.concatTitle,...detached}}
   })}
  })
  if(!found)throw new Error('Титр не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...target,content:{...target.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,nestedComposition.timeUs)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function insertNestedConcatTitle(compositionId:string,request:import('$lib/composition/nativeTitleInsert').NativeTitleInsert,validateCommit?:()=>void):Promise<void>{
 const submitted={...request},guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard()
  const {insertNativeConcatTitle}=await import('$lib/composition/nativeTitleInsert')
  const changed=insertNativeConcatTitle(document,compositionId,submitted)
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,submitted.startUs)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function editNestedConcatShape(compositionId:string,trackId:string,clipId:string,patch:Partial<import('$lib/composition/types').WireConcatShapeStyle>,blend:import('$lib/composition/types').CompositionBlendMode|undefined=undefined,validateCommit?:()=>void):Promise<void>{
 const detached=JSON.parse(JSON.stringify(patch)) as typeof patch,guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard();const target=document.compositions[compositionId];if(!target)throw new Error('Композиция не найдена')
  let found=false
  const tracks=target.content.tracks.map(track=>{
   if(track.id!==trackId||track.kind!=='graphic')return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   return {...track,clips:track.clips.map(clip=>{
    if(clip.id!==clipId)return clip
    if(!clip.concatShape)throw new Error('Выбранный клип не является фигурой Concat')
    found=true;return {...clip,concatShape:{...clip.concatShape,...detached},...(blend!==undefined?{concatShapeBlend:blend}:{})}
   })}
  })
  if(!found)throw new Error('Фигура не найдена')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...target,content:{...target.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,nestedComposition.timeUs);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function insertNestedConcatShape(compositionId:string,request:import('$lib/composition/nativeShapeInsert').NativeShapeInsert,validateCommit?:()=>void):Promise<void>{
 const submitted={...request},guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard()
  const {insertNativeConcatShape}=await import('$lib/composition/nativeShapeInsert')
  const changed=insertNativeConcatShape(document,compositionId,submitted)
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,submitted.startUs);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function editNestedConcatTreatment(compositionId:string,trackId:string,clipId:string,patch:Partial<Pick<NonNullable<import('$lib/composition/types').WireGraphicClip['concatTreatment']>,'strength'|'fadeIn'|'fadeOut'>>,validateCommit?:()=>void):Promise<void>{
 const detached={...patch},guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard();const target=document.compositions[compositionId];if(!target)throw new Error('Композиция не найдена')
  let found=false
  const tracks=target.content.tracks.map(track=>{
   if(track.id!==trackId||track.kind!=='graphic')return track
   if(track.locked)throw new Error('Дорожка заблокирована')
   return {...track,clips:track.clips.map(clip=>{
    if(clip.id!==clipId)return clip
    if(!clip.concatTreatment)throw new Error('Выбранный клип не является корректирующим слоем Concat')
    found=true;return {...clip,concatTreatment:{...clip.concatTreatment,...detached}}
   })}
  })
  if(!found)throw new Error('Корректирующий слой не найден')
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...target,content:{...target.content,tracks}}}}
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,nestedComposition.timeUs);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function insertNestedConcatTreatment(compositionId:string,request:Omit<import('$lib/composition/nativeTreatmentInsert').NativeTreatmentInsert,'effects'>&{effectId:string},validateCommit?:()=>void):Promise<void>{
 const submitted={...request},guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard()
  if(!submitted.effectId.trim()||submitted.effectId.length>256)throw new Error('Выберите пакет эффекта')
  const {upgradeConcatEffectLinks,evaluateNestedFrame}=await import('$lib/api')
  const resolved=await upgradeConcatEffectLinks([{id:submitted.effectId,enabled:true}],undefined,true)
  const effect=resolved.nativeEffects?.[0]
  if(resolved.links.length!==1||!effect||effect.kind==='concat_bypass'||effect.kind==='native_bypass')throw new Error('Этот пакет пока нельзя применить к корректирующему слою')
  guard()
  const {insertNativeConcatTreatment}=await import('$lib/composition/nativeTreatmentInsert')
  const changed=insertNativeConcatTreatment(document,compositionId,{...submitted,effects:[effect]})
  await evaluateNestedFrame(changed,submitted.startUs);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function addNestedConcatTreatmentEffect(compositionId:string,trackId:string,clipId:string,effectId:string,validateCommit?:()=>void):Promise<void>{
 const guard=()=>{assertHistoryUnlocked();validateCommit?.()}
 await editNestedDocument(async document=>{
  guard();const target=document.compositions[compositionId],track=target?.content.tracks.find(track=>track.id===trackId)
  if(!target||!track||track.kind!=='graphic')throw new Error('Корректирующий слой не найден')
  if(track.locked)throw new Error('Дорожка заблокирована')
  const clip=track.clips.find(clip=>clip.id===clipId);if(!clip?.concatTreatment)throw new Error('Нужен корректирующий слой Concat')
  if(!effectId.trim()||effectId.length>256)throw new Error('Выберите пакет эффекта')
  const {upgradeConcatEffectLinks,evaluateNestedFrame}=await import('$lib/api')
  const resolved=await upgradeConcatEffectLinks([{id:effectId,enabled:true}],undefined,true),effect=resolved.nativeEffects?.[0]
  if(resolved.links.length!==1||!effect||effect.kind==='concat_bypass'||effect.kind==='native_bypass')throw new Error('Этот пакет пока нельзя применить к корректирующему слою')
  guard()
  const replacement={...track,clips:track.clips.map(current=>current.id===clipId?{...current,effects:[...(current.effects??[]),effect]}:current)}
  const changed={...document,compositions:{...document.compositions,[compositionId]:{...target,content:{...target.content,tracks:target.content.tracks.map(current=>current.id===trackId?replacement:current)}}}}
  await evaluateNestedFrame(changed,nestedComposition.timeUs);return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function quickPoseNestedClips(scope:string,ids:readonly string[],action:import('$lib/composition/nativeQuickPose').NativeQuickPose):Promise<void>{
 const selected=[...ids],time=nestedComposition.timeUs,guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{
  guard()
  const {quickPoseNativeClips}=await import('$lib/composition/nativeQuickPose')
  const changed=quickPoseNativeClips(document,scope,selected,time,action)
  if(changed===document)return document
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,time)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export async function clearNestedConcatKeys(scope:string,ids:readonly string[]):Promise<void>{
 const selected=[...ids],guard=()=>assertHistoryUnlocked()
 await editNestedDocument(async document=>{
  guard();const {clearNativeConcatKeys}=await import('$lib/composition/nativeClearConcatKeys')
  const changed=clearNativeConcatKeys(document,scope,selected);if(changed===document)return document
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,0)
  return changed
 },true,undefined,undefined,0,undefined,guard)
}

export function stepNestedConcatKey(scope:string,ids:readonly string[],direction:-1|1):void{
 if(nestedHistory.busy||agentSession.lock||!nestedComposition.document)return
 const document=nestedComposition.document,time=nestedComposition.timeUs,selected=[...ids]
 if(nestedComposition.document!==document||nestedComposition.timeUs!==time||nestedHistory.busy||agentSession.lock)return
 const target=concatSelectionKeyTarget(document,scope,selected,time,direction)
 if(target!==null){nestedPlayback.playing=false;nestedPlayback.epoch++;nestedComposition.timeUs=target}
}

export async function moveNestedConcatKeyMarks(scope:string,ids:readonly string[],from:number,to:number,expected:NativeNestedProject):Promise<void>{
 const selected=[...ids],guard=()=>{assertHistoryUnlocked();if(nestedComposition.document!==expected||(nestedComposition.activeCompositionId??expected.root)!==scope)throw Error('Композиция изменилась во время переноса ключей')}
 await editNestedDocument(async document=>{
  guard();const {moveNativeConcatKeys}=await import('$lib/composition/nativeMoveConcatKeys')
  const changed=moveNativeConcatKeys(document,scope,selected,from,to);if(changed===document)return document
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(changed,from);return changed
 },true,undefined,undefined,0,undefined,guard)
}

/** Preserve original source coverage while setting the normalized speed curve. */
export async function setNestedSourceSpeedCurve(id: string, ids: readonly string[], points: import('$lib/composition/concatSourceCurve').SourceSpeedPoints): Promise<void> {
  const captured = points?.map(point => ({ at: point.at, speed: point.speed })) ?? null
  let affected: readonly string[] = []
  await editNestedDocument(async document => {
    const { setNativeSourceSpeedCurve } = await import('$lib/composition/concatSourceCurve')
    const changed = await setNativeSourceSpeedCurve(document, id, ids, captured)
    if (changed === document) return document
    const { evaluateNestedFrame } = await import('$lib/api')
    await evaluateNestedFrame(changed, 0)
    affected = affectedTimelineIds(document, id)
    return changed
  }, false, undefined, outputs => Object.fromEntries(Object.entries(outputs).filter(([key]) => !affected.includes(key))))
}


/** A saved server edit becomes one local undo record; pending replies never replace local edits. */
export async function freezeNestedVideoClip(token:string,compositionId:string,clipId:string,options:{actor?:string;timeUs?:number;validate?:()=>void;activity?:ActivityToken;onPublished?:()=>void}={}):Promise<string|null>{
 const projectId=nestedComposition.projectId,revision=nestedComposition.revision,name=nestedComposition.name,before=snapshot(true),cursor=nestedComposition.timeUs,activeId=nestedComposition.activeCompositionId,timeUs=options.timeUs??cursor
 if(!token||nestedHistory.busy||!projectId||revision===null||!before||!Number.isSafeInteger(timeUs)||timeUs<0)return null
 const node=before.document.compositions[compositionId],target=node?.content.tracks.flatMap(track=>track.kind==='video'&&!track.locked?track.clips:[]).find(clip=>clip.id===clipId)
 if(!target)return null
 const currentGeneration=generation,identity=JSON.stringify(before.document)
 const current=()=>generation===currentGeneration&&nestedComposition.projectId===projectId&&nestedComposition.revision===revision&&nestedComposition.name===name&&nestedComposition.activeCompositionId===activeId&&nestedComposition.timeUs===cursor&&JSON.stringify(nestedComposition.document)===identity
 let serverCommitted=false
 nestedHistory.busy=true;nestedHistory.error=''
 try{
  assertHistoryUnlocked(options.actor);options.validate?.()
  const api=await import('$lib/api'),saved=await api.getNestedCompositionProject(projectId,token)
  if(!current())return null
  if(saved.revision!==revision||saved.name!==name||canonicalCutoutDocument(saved.document)!==canonicalCutoutDocument(before.document))throw new Error('Сначала сохраните изменения проекта и обновите его при конфликте версий')
  assertHistoryUnlocked(options.actor);options.validate?.()
  const result=await api.freezeCompositionProject(projectId,{revision,compositionId,clipId,time:timeUs/1000000},token)
  serverCommitted=result.changed
  if(serverCommitted)options.onPublished?.()
  if(!current()){
   if(generation===currentGeneration&&result.changed)nestedHistory.error='Стоп-кадр сохранён на сервере. Локальные изменения оставлены; обновите проект перед следующей записью'
   return null
  }
  assertHistoryUnlocked(options.actor);options.validate?.()
  if(!result.changed)return null
  const updated=result.project,document=updated.document
  if(updated.id!==projectId||updated.revision!==revision+1||!('compositions' in document)||document.root!==before.document.root||!document.compositions[compositionId])throw new Error('Сервер вернул другой проект или ревизию стоп-кадра')
  const content=document.compositions[compositionId]!.content
  const images=content.tracks.flatMap(track=>track.kind==='image'?track.clips:[]).filter(clip=>clip.sourceId===result.media.id)
  const still=images[0],source=content.sources[result.media.id]
  if(result.media.mediaType!=='image'||before.document.compositions[compositionId]!.content.sources[result.media.id]||images.length!==1||!still||still.timelineStartTick!==timeUs||still.durationTicks!==1000000||source?.kind!=='image'||source.hasAudio||source.durationTicks!==0)throw new Error('Сервер не подтвердил новый стоп-кадр')
  for(const [id,node] of Object.entries(before.document.compositions))if(id!==compositionId&&canonicalCutoutDocument(document.compositions[id])!==canonicalCutoutDocument(node))throw new Error('Сервер изменил другую композицию при вставке стоп-кадра')
  nestedHistory.past=[...nestedHistory.past.slice(-99),before];nestedHistory.future=[]
  linkActivityHistory(options.activity,identifyHistoryEntry(before));if(options.activity)linkActivityEntities(options.activity,nestedAffected(before.document,document))
  nestedComposition.document=document;nestedComposition.revision=updated.revision;nestedComposition.name=updated.name;nestedComposition.renderedOutputs={}
  return still.id
 }catch(error){if(generation===currentGeneration)nestedHistory.error=(serverCommitted?'Стоп-кадр сохранён на сервере. Локальное состояние оставлено; обновите проект перед следующей записью. ':'')+(error instanceof Error?error.message:String(error));return null}
 finally{if(generation===currentGeneration)nestedHistory.busy=false}
}

/** Native scalar key operations share the agent's authored tick contract and one history entry. */
export async function editNestedScalarKey(compositionId: string, operation: Record<string, unknown>, validateCommit?: () => void): Promise<void> {
  await editNestedScalarKeys(compositionId,[operation],validateCommit)
}

/** One complete validation and history entry for a multi-channel key gesture. */
export async function editNestedScalarKeys(compositionId:string,operations:readonly Record<string,unknown>[],validateCommit?:()=>void,signal?:AbortSignal,onCommit?:()=>void):Promise<boolean>{
  assertHistoryUnlocked()
  const copied=structuredClone(operations)
  const guard=()=>{assertHistoryUnlocked();if(signal?.aborted)throw new Error('Редактирование ключей отменено');validateCommit?.()}
  let committed=false
  await editNestedDocument(async document=>{
    guard()
    const {applyNativeKeyframeOperation}=await import('$lib/agent/nativeKeyframes')
    let changed=document
    for(const operation of copied)changed=applyNativeKeyframeOperation(changed,compositionId,operation)
    guard()
    const {evaluateNestedFrame}=await import('$lib/api')
    if(signal)await evaluateNestedFrame(changed,0,signal);else await evaluateNestedFrame(changed,0)
    return changed
  },true,undefined,undefined,0,()=>{committed=true;onCommit?.()},guard)
  return committed
}


/** Geometry stays in the native path operation; authoring commits once after full validation. */
export async function editNestedMotionPath(compositionId:string,operation:Record<string,unknown>,validateCommit?:()=>void,onCandidate?:(project:NativeNestedProject)=>void):Promise<void>{
  assertHistoryUnlocked()
  const copied=structuredClone(operation),guard=()=>{assertHistoryUnlocked();validateCommit?.()}
  await editNestedDocument(async document=>{
    const {applyNativePathOperation}=await import('$lib/agent/nativePath')
    guard()
    const changed=await applyNativePathOperation(document,compositionId,copied)
    guard()
    onCandidate?.(changed)
    const {evaluateNestedFrame}=await import('$lib/api')
    await evaluateNestedFrame(changed,0)
    return changed
  },true,undefined,undefined,0,undefined,guard)
}


/** Commit the inspected conversion without fitting again after the user reviews it. */
export async function commitNestedPositionConversion(compositionId:string,clipId:string,result:import('$lib/api').NativePositionConversionResult,prefix:string,validateCommit:()=>void,signal?:AbortSignal):Promise<boolean>{
  const guard=()=>{assertHistoryUnlocked();if(signal?.aborted)throw new Error('Конвертация отменена');validateCommit()}
  guard()
  const detached=structuredClone(result)
  let committed=false
  await editNestedDocument(async document=>{
    guard()
    const {nativePositionConversionCandidate}=await import('$lib/composition/nativePositionConversion')
    const candidate=nativePositionConversionCandidate(document,compositionId,clipId,detached,prefix)
    const {evaluateNestedFrame}=await import('$lib/api')
    await evaluateNestedFrame(candidate,0,signal)
    return candidate
  },true,undefined,undefined,0,()=>{committed=true},guard)
  return committed
}


/** Paste a reviewed key snapshot with full-project validation and one history entry. */
export async function pasteNestedScalarKeys(compositionId:string,clipId:string,clipboard:import('$lib/composition/nativeScalarClipboard').NativeScalarClipboard,timeUs:number,validateCommit:()=>void,signal?:AbortSignal):Promise<boolean>{
 const guard=()=>{assertHistoryUnlocked();if(signal?.aborted)throw new Error('Вставка ключей отменена');validateCommit()}
 guard();const detached=structuredClone(clipboard);let committed=false
 await editNestedDocument(async document=>{
  guard();const {planNativeScalarPaste}=await import('$lib/composition/nativeScalarClipboard')
  const candidate=planNativeScalarPaste(document,compositionId,detached,clipId,timeUs).project
  const {evaluateNestedFrame}=await import('$lib/api');await evaluateNestedFrame(candidate,0,signal);return candidate
 },true,undefined,undefined,0,()=>{committed=true},guard)
 return committed
}
