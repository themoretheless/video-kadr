import {nativeEffectScalarBindings} from '../composition/nativeEffectScalar'
import { keyframeLocalTime } from '../composition/keyframeClock'
import type {NativeNestedProject} from '../api'
import type {CompositionAnimatableValue,WireCompositionTrack} from '../composition/types'
import {MAX_KEYFRAMES_PER_VALUE} from '../composition/types'
import {upsertKeyframe,deleteKeyframe,updateSelectedKeyframeInterpolation} from '../composition/keyframes'
import {cloneKeyframe,normalizeKeyframeTangents,setKeyframeTangentMode,editKeyframeTangent,updateKeyframeTangents,resolvedKeyframeTangents,segmentInterpolation} from '../composition/tangents'
import {EASING_PRESETS,easingPresetId} from '../composition/easingPresets'
import {nativeClipStart} from '../composition/nativeClipMove'
export const NATIVE_KEYFRAME_OPERATIONS=['upsert_keyframe','delete_keyframe','move_keyframe','edit_keyframe','set_keyframe_preset','set_keyframe_tangent_mode','edit_keyframe_tangents','set_keyframe_extrapolation'] as const
export const NATIVE_KEYFRAME_ENVELOPES=['visual','audio','path','effect'] as const
type Row=Record<string,unknown>
function record(value:unknown):Row {if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Expected animation object');return value as Row}
function tick(value:unknown):number{if(!Number.isSafeInteger(value))throw new Error('Key tick must be a safe signed integer in the track timeBase');return value as number}
function finite(value:unknown):number{if(typeof value!=='number'||!Number.isFinite(value))throw new Error('Key value must be finite');return value}
function selected(value:unknown):number[]{if(!Array.isArray(value)||!value.length||value.length>MAX_KEYFRAMES_PER_VALUE||new Set(value).size!==value.length)throw new Error('Select distinct existing keys');return value.map(tick)}
function preset(value:unknown){const found=EASING_PRESETS.find(entry=>entry.id===value);if(!found)throw new Error('Unknown easing preset');return found.interpolation}
function key(value:CompositionAnimatableValue,at:number){if(value.mode!=='keyframes')throw new Error('Animation track not found');const found=value.track.keyframes.find(key=>key.tick===at);if(!found)throw new Error('Selected keyframe no longer exists');return found}
function move(value:CompositionAnimatableValue,from:number,to:number):CompositionAnimatableValue {
 key(value,from);if(value.mode!=='keyframes')throw new Error('Animation track not found')
 if(from===to)return value
 if(value.track.keyframes.some(key=>key.tick===to))throw new Error('Another key already occupies that exact time')
 const keys=value.track.keyframes.map(key=>({...cloneKeyframe(key),tick:key.tick===from?to:key.tick})).sort((a,b)=>a.tick-b.tick)
 return {...value,track:{...value.track,keyframes:normalizeKeyframeTangents(keys,value.track.interpolation)}}
}
function tangents(value:CompositionAnimatableValue,op:Row,at:number):CompositionAnimatableValue {
 key(value,at)
 if(op.incoming===undefined&&op.outgoing===undefined&&op.continuity===undefined)throw new Error('Tangent edit requires a change')
 let next=value
 for(const side of ['incoming','outgoing'] as const)if(op[side]!==undefined){const point=record(op[side]);if(Object.keys(point).some(key=>key!=='x'&&key!=='y'))throw new Error('Invalid tangent point');next=editKeyframeTangent(next,at,side,finite(point.x),finite(point.y))}
 if(op.continuity!==undefined){if(op.continuity!=='smooth'&&op.continuity!=='broken')throw new Error('Invalid tangent continuity');next=updateKeyframeTangents(next,[at],{continuity:op.continuity})}
 return next
}
function change(value:CompositionAnimatableValue,op:Row):CompositionAnimatableValue{
 const at=op.tick===undefined?undefined:tick(op.tick)
 switch(op.op){
  case 'set_keyframe_extrapolation':{
   if(value.mode!=='keyframes')throw new Error('Animation track not found')
   if(op.side!=='before'&&op.side!=='after'||typeof op.extrapolation!=='string'||!['hold','continue','loop','ping_pong','offset'].includes(op.extrapolation))throw new Error('Invalid extrapolation')
   return {...value,track:{...value.track,extrapolation:{before:'hold',after:'hold',...value.track.extrapolation,[op.side]:op.extrapolation as import('../composition/types').CompositionExtrapolationMode}}}
  }
  case 'upsert_keyframe':return upsertKeyframe(value,tick(op.tick),finite(op.value),value.mode==='constant'?value.value:finite(op.value),op.preset===undefined?undefined:preset(op.preset))
  case 'delete_keyframe':key(value,tick(op.tick));return deleteKeyframe(value,tick(op.tick))!
  case 'move_keyframe':return move(value,tick(op.tick),tick(op.newTick))
  case 'set_keyframe_preset':return updateSelectedKeyframeInterpolation(value,selected(op.ticks),preset(op.preset))
  case 'set_keyframe_tangent_mode':if(op.mode!=='auto'&&op.mode!=='free')throw new Error('Invalid tangent mode');return setKeyframeTangentMode(value,selected(op.ticks),op.mode)
  case 'edit_keyframe_tangents':return tangents(value,op,tick(op.tick))
  case 'edit_keyframe':{
   key(value,tick(op.tick))
   if(!['newTick','value','preset','incoming','outgoing','continuity'].some(field=>op[field]!==undefined))throw new Error('Keyframe edit requires a change')
   const destination=op.newTick===undefined?at!:tick(op.newTick)
   let next=move(value,at!,destination)
   if(op.value!==undefined)next=upsertKeyframe(next,destination,finite(op.value),finite(op.value))
   if(op.preset!==undefined)next=updateSelectedKeyframeInterpolation(next,[destination],preset(op.preset))
   if(op.incoming!==undefined||op.outgoing!==undefined||op.continuity!==undefined)next=tangents(next,op,destination)
   return next
  }
  default:throw new Error('Unsupported native keyframe operation')
 }
}
function visualPath(clip:Row,property:string):string[]{
 if(property==='opacity')return ['opacity']
 if(property==='x'||property==='y'){
  const position=record(clip.transform).position
  if(position!==undefined){const mode=record(position).mode;if(mode==='xy')return ['transform','position',property];if(mode==='path')throw new Error('Active path uses progress; XY envelopes are inactive')}
 }
 return ['transform',property]
}
function binding(project:NativeNestedProject,scope:string,args:Row){
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 const track=node?.content.tracks.find(track=>track.clips.some(clip=>clip.id===args.clipId))
 if(!node||!track)throw new Error('Layer not found in composition')
 const clip=track.clips.find(clip=>clip.id===args.clipId)!,envelope=args.envelope??'visual',property=args.property
 if(args.maskId!==undefined||envelope==='mask')throw new Error('Native mask effect identities are not available through this interface')
 if(envelope!=='effect'&&(args.effectIndex!==undefined||args.effectKind!==undefined))throw new Error('Effect address requires effect envelope')
 let path:string[],fallback=0
 if(envelope==='visual'){
  if(track.kind==='audio'||!['x','y','scaleX','scaleY','rotationDegrees','opacity'].includes(String(property)))throw new Error('Unknown native visual animation property')
  path=visualPath(record(clip),String(property));fallback=['scaleX','scaleY','opacity'].includes(String(property))?1:0
 }else if(envelope==='audio'){
  if(!['audio','video'].includes(track.kind)||property!=='gain'&&property!=='pan')throw new Error('Unknown native audio animation property')
  path=[track.kind==='video'?(property==='gain'?'audioGain':'audioPan'):property];fallback=property==='gain'?1:0
 }else if(envelope==='path'){
  if(property!=='progress'||record(record(clip).transform).position===undefined||record(record(record(clip).transform).position).mode!=='path')throw new Error('Native path progress required')
  path=['transform','position','progress']
 }else if(envelope==='effect'){
  if(!Number.isSafeInteger(args.effectIndex)||typeof args.effectKind!=='string')throw new Error('Effect address requires exact index and kind')
  const target=nativeEffectScalarBindings(clip).find(item=>item.effectIndex===args.effectIndex&&item.effectKind===args.effectKind&&item.property===property)
  if(!target)throw new Error('Active native effect animation channel not found')
  path=['effects',String(target.effectIndex),...target.path]
 }else throw new Error('Unknown animation envelope')
 let current:unknown=clip
 for(const field of path)current=current===undefined?undefined:Array.isArray(current)?current[Number(field)]:record(current)[field]
 const value=(current??{mode:'constant',value:fallback}) as CompositionAnimatableValue
 if(value.mode!=='constant'&&value.mode!=='keyframes')throw new Error('Unsupported native animation value')
 return {node,track,clip,path,value}
}
function replace(object:unknown,path:readonly string[],value:CompositionAnimatableValue):unknown{
 if(!path.length)return value
 if(Array.isArray(object)){const index=Number(path[0]);if(!Number.isSafeInteger(index)||index<0||index>=object.length)throw new Error('Effect index changed');return object.map((item,at)=>at===index?replace(item,path.slice(1),value):item)}
 const before=object===undefined?{}:record(object),[field,...rest]=path
 return {...before,[field!]:replace(before[field!],rest,value)}
}
export function applyNativeKeyframeOperation(project:NativeNestedProject,scope:string,op:Row):NativeNestedProject{
 const extras:Record<string,readonly string[]>={set_keyframe_extrapolation:['side','extrapolation'],upsert_keyframe:['tick','value','preset'],delete_keyframe:['tick'],move_keyframe:['tick','newTick'],edit_keyframe:['tick','newTick','value','preset','incoming','outgoing','continuity'],set_keyframe_preset:['ticks','preset'],set_keyframe_tangent_mode:['ticks','mode'],edit_keyframe_tangents:['tick','incoming','outgoing','continuity']}
 const allowed=['op','clipId','property','envelope','effectIndex','effectKind',...(extras[String(op.op)]??[])]
 if(Object.keys(op).some(field=>!allowed.includes(field)))throw new Error('Unsupported native keyframe argument')
 const {track,clip,value}=binding(project,scope,op)
 if(track.locked)throw new Error('Track is locked')
 return replaceNativeKeyframeValue(project,scope,{clipId:clip.id,envelope:op.envelope,property:op.property,effectIndex:op.effectIndex,effectKind:op.effectKind},change(value,op))
}
/** Internal authoring boundary; callers must validate the complete candidate before commit. */
export function replaceNativeKeyframeValue(project:NativeNestedProject,scope:string,args:Row,value:CompositionAnimatableValue):NativeNestedProject{
 const {node,track,clip,path}=binding(project,scope,args)
 if(track.locked)throw new Error('Track is locked')
 const changed=replace(clip,path,value) as typeof clip
 return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clip.id?changed:item)} as WireCompositionTrack:row)}}}}
}

export function readNativeKeyframes(project:NativeNestedProject,scope:string,args:Row){
 if(Object.keys(args).some(field=>!['composition_id','clipId','property','envelope','maskId','effectIndex','effectKind'].includes(field)))throw new Error('Invalid keyframe read argument')
 const {clip,value}=binding(project,scope,args)
 if(value.mode==='constant')return {mode:'constant' as const,value:value.value}
 const track=value.track,origin=args.envelope==='effect'?nativeEffectScalarBindings(clip).find(item=>item.effectIndex===args.effectIndex&&item.effectKind===args.effectKind&&item.property===args.property)?.shift??0:0
 return {...(args.envelope==='effect'?{keyOriginUs:nativeClipStart(clip)+origin}:{}),mode:'keyframes' as const,timeBase:track.timeBase,...(track.sampleClock?{sampleClock:{...track.sampleClock}}:{}),interpolation:structuredClone(track.interpolation),...(track.extrapolation?{extrapolation:{...track.extrapolation}}:{}),keyframes:track.keyframes.map((key,index)=>{
  const next=track.keyframes[index+1],segment=next?segmentInterpolation(key,next,track.interpolation):undefined,preset=segment?easingPresetId(segment):undefined
  const local=keyframeLocalTime(track,key.tick)+origin
  return {tick:key.tick,t_local_us:local,t_us:nativeClipStart(clip)+local,value:key.value,...resolvedKeyframeTangents(track.keyframes,index,track.interpolation),...(segment?{segment:structuredClone(segment)}:{}),...(preset?{preset_id:preset}:{})}
 })}
}

export function setNativeVisualConstant<T extends WireCompositionTrack['clips'][number]>(clip:T,property:string,value:number):T{
 return replace(clip,visualPath(record(clip),property),{mode:'constant',value}) as T
}

/** Read the original curve for sampling; inspection rows carry presentation metadata. */
export function readNativeKeyframeValue(project: NativeNestedProject, scope: string, args: Row): CompositionAnimatableValue {
 return binding(project, scope, args).value
}
