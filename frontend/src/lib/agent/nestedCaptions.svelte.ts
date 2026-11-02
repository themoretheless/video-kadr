import {describeHistoryStacks} from './historyNavigation'
import {jumpNestedHistory} from '../state/nestedComposition.svelte'
import {readNativeColorTrack} from './nativeColor'
import {NATIVE_KEYFRAME_ENVELOPES,readNativeKeyframes} from './nativeKeyframes'
import {nestedDurations} from '../api'
import {evaluateNativeAgentBatch,NATIVE_BATCH_OPERATIONS,NATIVE_LAYER_PATCH_FIELDS,NATIVE_TEXT_STYLE_PATCH_FIELDS,NATIVE_CONCAT_TITLE_PATCH_FIELDS} from './nativeBatch'
import {linkActivityCheckpoint} from './activity.svelte'
import {SvelteDate} from 'svelte/reactivity'
import {createNestedCheckpoint,listNestedCheckpoints,readNestedCheckpoint,deleteNestedCheckpoint} from './nestedCheckpoints.svelte'
import {freezeNestedVideoClip,nestedComposition,nestedHistory,nestedAgentGeneration,applyNestedAgentDocument,undoNestedComposition,redoNestedComposition} from '../state/nestedComposition.svelte'
import {agentSession,assertHistoryUnlocked,beginAgentWork,endAgentWork} from './session.svelte'
import {restyleNestedCaptions,type CaptionStylePatch} from '../composition/restyleCaptions'
import {mergeNestedCaptions} from '../composition/mergeCaptions'
import type {AgentAccess} from './runtime.svelte'
import type {NativeNestedProject} from '../api'
const identities=new WeakMap<object,number>()
let nextIdentity=0
const session=crypto.randomUUID()
function capture(){
 const reference=nestedComposition.document
 if(!reference)throw new Error('Nested project is closed')
 if(!identities.has(reference))identities.set(reference,++nextIdentity)
 const context={session,generation:nestedAgentGeneration(),identity:identities.get(reference),projectId:nestedComposition.projectId,revision:nestedComposition.revision}
 const document=JSON.parse(JSON.stringify(reference)) as NativeNestedProject
 return {reference,context,document,serialized:JSON.stringify(document)}
}
async function hash(snapshot:ReturnType<typeof capture>){
 const bytes=new TextEncoder().encode(JSON.stringify({...snapshot.context,document:snapshot.document}))
 return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('')
}
function text(value:unknown,name:string):string{if(typeof value!=='string'||!value||value.length>128)throw new Error(`Invalid ${name}`);return value}
export async function executeNestedCaptionRequest(actor:string,tool:string,args:Record<string,unknown>,access:AgentAccess){
 const active=()=>{
  if(!actor||actor.length>128)throw new Error('Invalid actor')
  if(access.isActive&&!access.isActive()||access.expiresAt!==undefined&&access.expiresAt<=Date.now())throw new Error('Agent request expired or disconnected')
 }
 active()
 if(!nestedComposition.document&&tool==='redo'){
  assertHistoryUnlocked(actor)
  if(nestedHistory.busy)throw new Error('Nested project is busy')
  redoNestedComposition()
  return nestedComposition.document?{kind:'nested',projectHash:await hash(capture())}:{kind:'nested',closed:true,projectHash:null}
 }
 const snapshot=capture()
 if(tool==='get_project')return {nativeBatch:{operations:NATIVE_BATCH_OPERATIONS,layerPatchFields:NATIVE_LAYER_PATCH_FIELDS,textStylePatchFields:NATIVE_TEXT_STYLE_PATCH_FIELDS,concatTitlePatchFields:NATIVE_CONCAT_TITLE_PATCH_FIELDS,concatTitleBlends:['normal','multiply','screen','addition','lighten','darken'],effectsFormat:'native wire effects; replacement list preserves order; audio and ordinary text refuse',keyframeEnvelopes:NATIVE_KEYFRAME_ENVELOPES,moveSemantics:{move_layer:'first subject anchors time and destination; linked companions retain their lanes',move_layers:'exact requested set; first subject anchors time/destination, other subjects retain lanes and relative offsets',move_layer_placements:'exact per-subject lanes; anchorId controls common shift independent of array order'},newObjectFormat:'CompositionTrack/CompositionClip authoring fields from tools/list; retained document objects are native wire objects',limits:{perLayerLocks:false,escapeLink:['move_layer','trim_layer','split_layer','duplicate_layer'],duplicateTarget:'optional same-kind anchor track; linked companions retain lanes'},sourceSpeedCurve:{operation:'set_speed_curve',clipIds:'linked retained-clock AV clips',points:'null removes the curve; array of {at: normalized position, speed: multiplier}; preserves source coverage and stretches normalized keys without ripple'},scope:'composition_id defaults to root; integer microseconds'},workSession:agentSession.work?{...agentSession.work}:null,agentView:agentSession.view,kind:'nested',projectId:snapshot.context.projectId,name:nestedComposition.name,projectHash:await hash(snapshot),document:snapshot.document}
 if(tool==='get_color_track')return {kind:'nested',projectHash:await hash(snapshot),...readNativeColorTrack(snapshot.document,args.composition_id===undefined?snapshot.document.root:text(args.composition_id,'composition_id'),args)}
 if(tool==='get_keyframes')return {kind:'nested',projectHash:await hash(snapshot),...readNativeKeyframes(snapshot.document,args.composition_id===undefined?snapshot.document.root:text(args.composition_id,'composition_id'),args)}
 if(tool==='get_history'){
  const history={kind:'nested',...describeHistoryStacks(nestedHistory.past,nestedHistory.future),undoCount:nestedHistory.past.length,redoCount:nestedHistory.future.length,lock:agentSession.lock?{...agentSession.lock}:null,checkpoints:listNestedCheckpoints().map(({id,label,createdAt})=>({id,label,createdAt}))}
  return {...history,projectHash:await hash(snapshot)}
 }
 const createPoint=(label:string)=>{const point=createNestedCheckpoint(label,actor,snapshot.document);linkActivityCheckpoint(access.activity,point.id,point.label,'checkpoint');return point}
 if(tool==='begin_agent_session')return beginAgentWork(actor,args.reason,'nested',createPoint)
 if(tool==='end_agent_session')return endAgentWork(actor,args.force??false)
 if(tool==='list_checkpoints')return listNestedCheckpoints().map(item=>({id:item.id,label:item.label,actor:item.actor,created_at:new SvelteDate(item.createdAt).toISOString()}))
 if(tool==='create_checkpoint'){
  return createPoint(text(args.label,'label'))
 }
 if(tool==='delete_checkpoint'){
  const id=text(args.id,'id');deleteNestedCheckpoint(id);return {deleted:id}
 }
 if(tool==='jump_history'){
  if(Object.keys(args).some(field=>!['entryId','side','expectedProjectHash'].includes(field)))throw new Error('Invalid native history jump argument')
  if(args.expectedProjectHash!==await hash(snapshot))throw new Error('Project changed; read get_project again')
  active()
  if(nestedComposition.document!==snapshot.reference||nestedAgentGeneration()!==snapshot.context.generation||nestedComposition.projectId!==snapshot.context.projectId||nestedComposition.revision!==snapshot.context.revision)throw new Error('Project changed; read get_project again')
  const side=args.side??'after'
  if(side!=='before'&&side!=='after')throw new Error('Invalid history jump side')
  const steps=jumpNestedHistory(text(args.entryId,'entryId'),side,actor)
  const history={steps,...describeHistoryStacks(nestedHistory.past,nestedHistory.future)}
  return {...history,...(nestedComposition.document?{kind:'nested',projectHash:await hash(capture())}:{kind:'nested',closed:true,projectHash:null})}
 }
 if(tool==='undo'||tool==='redo'){
  assertHistoryUnlocked(actor)
  if(nestedHistory.busy)throw new Error('Nested project is busy')
  if(tool==='undo')undoNestedComposition();else redoNestedComposition()
  return nestedComposition.document ? {kind:'nested',projectHash:await hash(capture())} : {kind:'nested',closed:true,projectHash:null}
 }
 if(tool!=='freeze_frame'&&tool!=='merge_captions'&&tool!=='restyle_captions'&&tool!=='restore_checkpoint'&&tool!=='apply_batch'&&tool!=='dry_run')throw new Error('Tool is not available for an open nested project')
 if(args.expectedRevision!==undefined)throw new Error('Use expectedProjectHash for nested projects')
 if(typeof args.expectedProjectHash!=='string'||!/^[0-9a-f]{64}$/.test(args.expectedProjectHash)||await hash(snapshot)!==args.expectedProjectHash)throw new Error('Project changed; read get_project and use its projectHash as expectedProjectHash')
 const admission=()=>{
  active();assertHistoryUnlocked(actor)
  if(nestedComposition.document!==snapshot.reference||nestedAgentGeneration()!==snapshot.context.generation||nestedComposition.projectId!==snapshot.context.projectId||nestedComposition.revision!==snapshot.context.revision||JSON.stringify(nestedComposition.document)!==snapshot.serialized)throw new Error('Project changed; read get_project again')
 }
 admission()
 if(tool==='freeze_frame'){
  if(Object.keys(args).some(key=>!['expectedProjectHash','composition_id','clipId','time_us'].includes(key)))throw new Error('Invalid freeze_frame argument')
  const scope=args.composition_id===undefined?snapshot.document.root:text(args.composition_id,'composition_id')
  const clipId=text(args.clipId,'clipId')
  if(!Number.isSafeInteger(args.time_us)||(args.time_us as number)<0)throw new Error('Invalid time_us')
  const target=snapshot.document.compositions[scope]?.content.tracks.flatMap(track=>track.kind==='video'&&!track.locked?track.clips:[]).find(clip=>clip.id===clipId)
  if(!target)throw new Error('FreezeFrame requires an unlocked native video clip in this composition')
  if(!access.token||!snapshot.context.projectId||snapshot.context.revision===null)throw new Error('FreezeFrame requires an authenticated saved native project')
  if(nestedHistory.busy)throw new Error('Nested project is busy')
  let published=false
  const still=await freezeNestedVideoClip(access.token,scope,clipId,{actor,timeUs:args.time_us as number,validate:admission,activity:access.activity,onPublished:()=>{published=true}})
  if(nestedHistory.error)throw new Error(nestedHistory.error)
  if(still===null&&published)throw new Error('Стоп-кадр сохранён на сервере. Контекст редактора изменился; обновите проект перед следующей записью')
  if(still===null)admission()
  return {kind:'nested',changed:still!==null,stillId:still,projectHash:await hash(capture()),revision:nestedComposition.revision}
 }
 if(tool==='apply_batch'||tool==='dry_run'){
  if(Object.keys(args).some(key=>!['expectedProjectHash','composition_id','operations'].includes(key)))throw new Error('Invalid native batch argument')
  const scope=args.composition_id===undefined?snapshot.document.root:text(args.composition_id,'composition_id')
  const document=await evaluateNativeAgentBatch(snapshot.document,scope,args.operations)
  admission()
  if(tool==='dry_run'){await nestedDurations(document);admission()}
  else await applyNestedAgentDocument(snapshot.serialized,document,admission,undefined,access.activity)
  return {kind:'nested',dryRun:tool==='dry_run',changed:JSON.stringify(document)!==snapshot.serialized,projectHash:await hash(capture()),document}
 }
 let checkpoint:{id:string;label:string}|undefined
 let document:NativeNestedProject,result:Record<string,unknown>,selection:{id:string;timeUs:number}|undefined
 if(tool==='restore_checkpoint'){
  const id=text(args.id,'id')
  document=readNestedCheckpoint(id);result={id,restored:id}
  checkpoint=listNestedCheckpoints().find(point=>point.id===id)
  const activeId=nestedComposition.activeCompositionId??document.root
  const found=Object.hasOwn(document.compositions,activeId)
  selection={id:found?activeId:document.root,timeUs:found?nestedComposition.timeUs:0}
 }else if(tool==='restyle_captions'){
  const {expectedProjectHash,expectedRevision,layer_ids,...patch}=args
  void expectedProjectHash;void expectedRevision
  const change=restyleNestedCaptions(snapshot.document,patch as CaptionStylePatch,layer_ids as string[]|null|undefined)
  document=change.document;result={captions:change.captions,restyled:change.restyled}
 }else{
  const ids=args.layer_ids
  if(!Array.isArray(ids)||ids.length<2||ids.length>512||ids.some(id=>typeof id!=='string'))throw new Error('Invalid layer_ids')
  let compositionId=args.composition_id==null?undefined:text(args.composition_id,'composition_id')
  if(!compositionId){
   const candidates=Object.entries(snapshot.document.compositions).filter(([,node])=>ids.every(id=>node.content.tracks.some(track=>track.clips.some(clip=>clip.id===id))))
   if(candidates.length!==1)throw new Error('Captions must identify one composition; use composition_id for ambiguous IDs')
   compositionId=candidates[0]![0]
  }
  const change=mergeNestedCaptions(snapshot.document,compositionId,ids as string[])
  document=change.document;result=change.record
 }
 await applyNestedAgentDocument(snapshot.serialized,document,admission,selection,access.activity)
 if(checkpoint)linkActivityCheckpoint(access.activity,checkpoint.id,checkpoint.label,'restore')
 return {...result,kind:'nested',projectHash:await hash(capture())}
}
