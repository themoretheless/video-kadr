import {EASING_PRESETS} from '../composition/easingPresets'
import {isBezierInterpolation} from '../composition/bezier'
import {isElasticInterpolation} from '../composition/elastic'
import type {CompositionInterpolation} from '../composition/types'
import {nativeClipEnd} from '../composition/nativeClipInsert'
import {nativeColorToHex,packColorAnimation,colorToNative} from '../composition/colorAnimation'
import type {WireCompositionTrack} from '../composition/types'
import type {NativeColor,NativeColorTrack,NativeNestedProject} from '../api'
import {nativeClipStart} from '../composition/nativeClipMove'

function baseColor(clip:WireCompositionTrack['clips'][number]):NativeColor|undefined{
 if('concatTitle' in clip&&clip.concatTitle)return colorToNative(clip.concatTitle.color??'#ffffff')
 if('style' in clip){const color=clip.style.color;return {r:Math.round(color.red*255),g:Math.round(color.green*255),b:Math.round(color.blue*255),a:Math.round(color.alpha*255)}}
 if('color' in clip&&'r' in clip.color)return clip.color
 return undefined
}

/** Inspect packed Rust color timing without quantizing fractional microseconds. */
export function readNativeColorTrack(project:NativeNestedProject,scope:string,args:Record<string,unknown>){
 if(Object.keys(args).some(field=>!['composition_id','clipId'].includes(field)))throw new Error('Invalid color read argument')
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 const track=node?.content.tracks.find(track=>track.clips.some(clip=>clip.id===args.clipId))
 if(!track)throw new Error('Layer not found in composition')
 const clip=track.clips.find(clip=>clip.id===args.clipId)!
 if(track.kind!=='text'&&track.kind!=='color')throw new Error('Color track requires a text or color layer')
 const animation='colorAnimation' in clip?clip.colorAnimation:undefined
 if(!animation){
  const color=baseColor(clip)
  if(!color)throw new Error('Color track requires a text or color layer')
  return {mode:'constant' as const,value:{...color}}
 }
 return {mode:'keyframes' as const,timeBase:1000000,extrapolation:structuredClone(animation.extrapolate),nativeTrack:structuredClone(animation),keyframes:animation.keys.map(key=>({tick:key.t_us,t_local_us:key.t_us,t_us:nativeClipStart(clip)+key.t_us,value:{...key.value},incoming:{x:key.in_[0],y:key.in_[1],mode:key.tangent_modes?.incoming??'free'},outgoing:{x:key.out[0],y:key.out[1],mode:key.tangent_modes?.outgoing??'free'},continuity:key.tangent_modes?.continuity??'broken',segment:structuredClone(key.segment),...(key.legacy_curve?{legacy_curve:key.legacy_curve}:{})}))}
}

function normalizeNativeColorAuto(keys:NativeColorTrack['keys']):NativeColorTrack['keys']{
 return keys.map((key,index)=>({...key,...(key.tangent_modes?.incoming==='auto'&&index>0&&keys[index-1]!.segment.kind==='Spline'&&!keys[index-1]!.legacy_curve?{in_:[2/3,2/3] as const}:{}),...(key.tangent_modes?.outgoing==='auto'&&index+1<keys.length&&key.segment.kind==='Spline'&&!key.legacy_curve?{out:[1/3,1/3] as const}:{})}))
}

export const NATIVE_COLOR_OPERATIONS=['set_color_key','delete_color_key','edit_color_key','set_color_extrapolation','move_color_keys','set_color_interpolation','set_color_preset','set_color_tangent_mode','edit_color_tangents'] as const
export function applyNativeColorOperation(project:NativeNestedProject,scope:string,op:Record<string,unknown>):NativeNestedProject{
 if(op.op==='move_color_keys')return moveNativeColorKeys(project,scope,op)
 const allowed=op.op==='set_color_tangent_mode'?['op','clipId','tick','mode']:op.op==='edit_color_tangents'?['op','clipId','tick','side','point','continuity']:op.op==='set_color_preset'?['op','clipId','preset','tick']:op.op==='set_color_interpolation'?['op','clipId','interpolation','tick']:op.op==='set_color_extrapolation'?['op','clipId','side','extrapolation']:op.op==='edit_color_key'?['op','clipId','tick','newTick','value']:op.op==='set_color_key'?['op','clipId','playheadTicks','value']:['op','clipId','tick']
 if(Object.keys(op).some(field=>!allowed.includes(field)))throw new Error('Unsupported native color argument')
 readNativeColorTrack(project,scope,{clipId:op.clipId})
 const node=project.compositions[scope]!,track=node.content.tracks.find(row=>row.clips.some(clip=>clip.id===op.clipId))!,clip=track.clips.find(row=>row.id===op.clipId)!
 if(track.locked)throw new Error('Track is locked')
 let animation:NativeColorTrack|undefined='colorAnimation' in clip?clip.colorAnimation:undefined
 let constant:NativeColor|undefined
 const keys=[...(animation?.keys??[])]
 if(op.op==='set_color_tangent_mode'||op.op==='edit_color_tangents'){
  if(typeof op.tick!=='number'||!Number.isFinite(op.tick)||Math.abs(op.tick)>Number.MAX_SAFE_INTEGER)throw new Error('Invalid native color key time')
  const index=keys.findIndex(key=>key.t_us===op.tick)
  if(index<0||!animation)throw new Error('Color key does not exist')
  const key=keys[index]!,modes={incoming:'free' as const,outgoing:'free' as const,continuity:'broken' as const,...key.tangent_modes}
  const spline=(span:number)=>{
   if(span<0||span+1>=keys.length)return
   const {legacy_curve:legacy,...left}=keys[span]!
   if(left.segment.kind!=='Spline'||legacy){keys[span+1]={...keys[span+1]!,in_:[2/3,2/3]};keys[span]={...left,out:[1/3,1/3],segment:{kind:'Spline'}}}
   else keys[span]={...left,segment:{kind:'Spline'}}
  }
  if(op.op==='set_color_tangent_mode'){
   if(op.mode!=='auto'&&op.mode!=='free')throw new Error('Invalid color tangent mode')
   keys[index]={...key,tangent_modes:{...modes,incoming:op.mode,outgoing:op.mode,...(op.mode==='auto'?{continuity:'smooth' as const}:{})}}
   if(op.mode==='auto'){
    spline(index-1);spline(index)
    keys[index]={...keys[index]!,...(index>0?{in_:[2/3,2/3] as const}:{}),...(index+1<keys.length?{out:[1/3,1/3] as const}:{})}
   }
  }else{
   if(op.continuity!==undefined&&op.continuity!=='smooth'&&op.continuity!=='broken')throw new Error('Invalid color tangent continuity')
   if(op.point===undefined&&op.continuity===undefined)throw new Error('Color tangent edit requires a change')
   if(op.side!==undefined&&op.side!=='incoming'&&op.side!=='outgoing')throw new Error('Invalid color tangent side')
   keys[index]={...key,tangent_modes:{...modes,...(op.continuity===undefined?{}:{continuity:op.continuity})}}
   if(op.point!==undefined){
    const point=op.point as {x:number;y:number}
    if(!point||typeof point!=='object'||Array.isArray(point)||Object.keys(point).some(field=>!['x','y'].includes(field))||typeof point.x!=='number'||!Number.isFinite(point.x)||point.x<0||point.x>1||typeof point.y!=='number'||!Number.isFinite(point.y)||op.side!=='incoming'&&op.side!=='outgoing')throw new Error('Invalid color tangent point')
    spline(op.side==='incoming'?index-1:index)
    keys[index]={...keys[index]!,[op.side==='incoming'?'in_':'out']:[point.x,point.y],tangent_modes:{...keys[index]!.tangent_modes!,incoming:'free',outgoing:'free'}}
   }
  }
  animation={...animation,keys}
 }else if(op.op==='set_color_interpolation' ||op.op==='set_color_preset'){
  if(!animation||!keys.length)throw new Error('Color keys required')
  let interpolation:unknown=op.interpolation
  if(op.op==='set_color_preset'){
   const entry=EASING_PRESETS.find(preset=>preset.id===op.preset)
   if(!entry)throw new Error('Unknown color easing preset')
   interpolation=entry.interpolation
  }
  if(!isBezierInterpolation(interpolation)&&!isElasticInterpolation(interpolation)&&!(typeof interpolation==='string'&&['hold','linear','ease_in','ease_out','ease_in_out','ease_in_out_cubic','bounce_in','bounce_out','bounce_in_out'].includes(interpolation)))throw new Error('Invalid color interpolation')
  if(op.tick!==undefined&&(typeof op.tick!=='number'||!Number.isFinite(op.tick)||Math.abs(op.tick)>Number.MAX_SAFE_INTEGER||!keys.some(key=>key.t_us===op.tick)))throw new Error('Color key does not exist')
  const packed=packColorAnimation({mode:'keyframes',track:{timeBase:1000000,interpolation:interpolation as CompositionInterpolation,keyframes:[{tick:0,value:'#000000FF'},{tick:1,value:'#FFFFFFFF'}]}})
  for(let index=0;index+1<keys.length;index++){
   if(op.tick!==undefined&&keys[index]!.t_us!==op.tick)continue
   const {legacy_curve:previousLegacy,...left}=keys[index]!
   void previousLegacy
   keys[index]={...left,...(left.tangent_modes?{tangent_modes:{...left.tangent_modes,outgoing:'free' as const,continuity:'broken' as const}}:{}),segment:packed.keys[0]!.segment,out:packed.keys[0]!.out,...(packed.keys[0]!.legacy_curve?{legacy_curve:packed.keys[0]!.legacy_curve}:{})}
   keys[index+1]={...keys[index+1]!,...(keys[index+1]!.tangent_modes?{tangent_modes:{...keys[index+1]!.tangent_modes!,incoming:'free' as const,continuity:'broken' as const}}:{}),in_:packed.keys[1]!.in_}
  }
  animation={...animation,keys}
 }else if(op.op==='set_color_extrapolation'){
  const modes={hold:'Hold',continue:'Continue',loop:'Loop',ping_pong:'PingPong',offset:'Offset'} as const
  if(op.side!=='before'&&op.side!=='after'||typeof op.extrapolation!=='string'||!Object.hasOwn(modes,op.extrapolation))throw new Error('Invalid color extrapolation')
  if(!animation||!keys.length)throw new Error('Color keys required')
  animation={...animation,extrapolate:{...animation.extrapolate,[op.side]:modes[op.extrapolation as keyof typeof modes]}}
 }else if(op.op==='set_color_key'){
  nativeColorToHex(op.value as NativeColor)
  const playhead=op.playheadTicks
  if(!Number.isSafeInteger(playhead)||Number(playhead)<nativeClipStart(clip)||Number(playhead)>nativeClipEnd(clip))throw new Error('Color key time is outside the layer')
  const at=Number(playhead)-nativeClipStart(clip)
  const makeKey=(t_us:number,value:NativeColor):NativeColorTrack['keys'][number]=>({t_us,value:{...value},in_:[2/3,2/3],out:[1/3,1/3],segment:{kind:'Linear'}})
  if(!keys.length&&at!==0){
   const base=baseColor(clip)
   if(!base)throw new Error('Native color base required')
   keys.push(makeKey(0,base))
  }
  const index=keys.findIndex(key=>key.t_us===at)
  if(index<0)keys.push(makeKey(at,op.value as NativeColor));else keys[index]={...keys[index]!,value:{...op.value as NativeColor}}
  if(keys.length>4096)throw new Error('Color animation accepts at most 4096 keys')
  keys.sort((a,b)=>a.t_us-b.t_us)
  animation={keys,extrapolate:animation?.extrapolate??{before:'Hold',after:'Hold'}}
 }else if(op.op==='edit_color_key'){
  const validTime=(value:unknown):value is number=>typeof value==='number'&&Number.isFinite(value)&&Math.abs(value)<=Number.MAX_SAFE_INTEGER
  if(!validTime(op.tick)||op.newTick!==undefined&&!validTime(op.newTick))throw new Error('Invalid native color key time')
  const index=keys.findIndex(key=>key.t_us===op.tick)
  if(index<0)throw new Error('Color key does not exist')
  if(op.newTick===undefined&&op.value===undefined)throw new Error('Color key edit requires a change')
  const destination=op.newTick===undefined?op.tick:op.newTick as number
  if(keys.some((key,i)=>i!==index&&key.t_us===destination))throw new Error('Another color key occupies that exact time')
  if(op.value!==undefined)nativeColorToHex(op.value as NativeColor)
  keys[index]={...keys[index]!,t_us:destination,...(op.value===undefined?{}:{value:{...op.value as NativeColor}})}
  keys.sort((a,b)=>a.t_us-b.t_us)
  animation={...animation!,keys}
 }else if(op.op==='delete_color_key'){
  if(typeof op.tick!=='number'||!Number.isFinite(op.tick)||Math.abs(op.tick)>Number.MAX_SAFE_INTEGER)throw new Error('Invalid native color key time')
  const index=keys.findIndex(key=>key.t_us===op.tick)
  if(index<0)throw new Error('Color key does not exist')
  const [removed]=keys.splice(index,1)
  if(keys.length)animation={...animation!,keys};else {animation=undefined;constant=removed!.value}
 }else throw new Error('Unsupported native color operation')
 const changed={...clip} as typeof clip & {colorAnimation?:NativeColorTrack}
 if(animation)changed.colorAnimation={...animation,keys:normalizeNativeColorAuto(animation.keys)}
 else {
  delete changed.colorAnimation
  if(constant&&'concatTitle' in changed&&changed.concatTitle)Object.assign(changed,{concatTitle:{...changed.concatTitle,color:nativeColorToHex(constant)}})
  else if(constant&&'style' in changed)Object.assign(changed,{style:{...changed.style,color:{red:constant.r/255,green:constant.g/255,blue:constant.b/255,alpha:constant.a/255}}})
  else if(constant)Object.assign(changed,{color:{...constant}})
 }
 return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clip.id?changed:item)} as WireCompositionTrack:row)}}}}
}

function moveNativeColorKeys(project:NativeNestedProject,scope:string,op:Record<string,unknown>):NativeNestedProject{
 if(Object.keys(op).some(field=>!['op','edits'].includes(field)))throw new Error('Unsupported native color move argument')
 if(!Array.isArray(op.edits)||!op.edits.length||op.edits.length>4096)throw new Error('Color key move requires 1..4096 subjects')
 const groups=new Map<string,Map<number,number>>()
 for(const item of op.edits){
  if(!item||typeof item!=='object'||Array.isArray(item)||Object.keys(item).some(field=>!['clipId','tick','newTick'].includes(field)))throw new Error('Invalid color key move subject')
  const {clipId,tick,newTick}=item
  if(typeof clipId!=='string'||!clipId||[tick,newTick].some(value=>typeof value!=='number'||!Number.isFinite(value)||Math.abs(value)>Number.MAX_SAFE_INTEGER))throw new Error('Invalid color key move subject')
  let edits=groups.get(clipId)
  if(!edits){edits=new Map();groups.set(clipId,edits)}
  if(edits.has(tick))throw new Error('Duplicate color key move subject')
  edits.set(tick,newTick)
 }
 const node=project.compositions[scope],replacements=new Map<string,NativeColorTrack>()
 for(const [clipId,edits] of groups){
  const inspected=readNativeColorTrack(project,scope,{clipId})
  const track=node!.content.tracks.find(row=>row.clips.some(clip=>clip.id===clipId))!
  if(track.locked)throw new Error('Track is locked')
  if(inspected.mode!=='keyframes')throw new Error('Color keys required')
  const animation=inspected.nativeTrack
  for(const tick of edits.keys())if(!animation.keys.some(key=>key.t_us===tick))throw new Error('Color key does not exist')
  const keys=animation.keys.map(key=>edits.has(key.t_us)?{...key,t_us:edits.get(key.t_us)!}:key).sort((a,b)=>a.t_us-b.t_us)
  if(new Set(keys.map(key=>key.t_us)).size!==keys.length)throw new Error('Another color key occupies that exact time')
  replacements.set(clipId,{...animation,keys:normalizeNativeColorAuto(keys)})
 }
 return {...project,compositions:{...project.compositions,[scope]:{...node!,content:{...node!.content,tracks:node!.content.tracks.map(track=>track.clips.some(clip=>replacements.has(clip.id))?{...track,clips:track.clips.map(clip=>replacements.has(clip.id)?{...clip,colorAnimation:replacements.get(clip.id)!}:clip)} as WireCompositionTrack:track)}}}}
}
