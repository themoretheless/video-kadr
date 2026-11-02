import {nativePositionConversionCandidate} from '../composition/nativePositionConversion'
import {keyframeAuthoredTick} from '../composition/keyframeClock'
import {previewPositionConversion} from '../api'
import {packPositionConversion} from '../composition/positionConversion'
import {evaluateMotionPath,type NativeMotionPathRequest} from '../api'
import type {NativeNestedProject} from '../api'
import type {CompositionPositionAnimation,WireCompositionTrack} from '../composition/types'
import {nativeClipStart} from '../composition/nativeClipMove'
import {nativeClipEnd} from '../composition/nativeClipInsert'
import {isStableId} from '../composition/validation'
import {applyNativeKeyframeOperation} from './nativeKeyframes'
export const NATIVE_PATH_OPERATIONS=['switch_position_mode','set_path_progress','edit_motion_path','convert_motion_path','arrive_at_path_node','insert_path_node_near'] as const
export async function applyNativePathOperation(project:NativeNestedProject,scope:string,op:Record<string,unknown>,signal?:AbortSignal):Promise<NativeNestedProject>{
 const geometry=async(request:NativeMotionPathRequest)=>{const response=signal?await evaluateMotionPath(request,signal):await evaluateMotionPath(request);if(signal?.aborted)throw new Error('Motion path edit cancelled');return response}
 if(signal?.aborted)throw new Error('Motion path edit cancelled')
 const allowed=op.op==='insert_path_node_near'?['op','clipId','point','nodeId','maxDistance']:op.op==='arrive_at_path_node'?['op','clipId','index','playheadTicks']:op.op==='convert_motion_path'?['op','clipId','nodeIdPrefix','options']:op.op==='edit_motion_path'?['op','clipId','edit']:op.op==='switch_position_mode'?['op','clipId','positionMode','nodeId']:['op','clipId','playheadTicks','value']
 if(Object.keys(op).some(field=>!allowed.includes(field)))throw new Error('Unsupported native path argument')
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 const track=node?.content.tracks.find(row=>row.clips.some(clip=>clip.id===op.clipId)),clip=track?.clips.find(clip=>clip.id===op.clipId)
 if(!node||!track||!clip||!('transform' in clip))throw new Error('Visual layer required')
 if(track.locked)throw new Error('Track is locked')
 const current=clip.transform.position
 if(op.op==='convert_motion_path'){
  if(!isStableId(op.nodeIdPrefix))throw new Error('Invalid conversion node ID prefix')
  const options=op.options as Record<string,unknown>
  if(!options||typeof options!=='object'||Array.isArray(options)||Object.keys(options).some(field=>!['startFrame','endFrame','tolerancePx','everyFrames','xyMode'].includes(field)))throw new Error('Invalid conversion options')
  const {startFrame,endFrame,tolerancePx,everyFrames,xyMode}=options
  const canvas=node.content.canvas,rate=canvas.fpsRate??{numerator:canvas.fpsMilli,denominator:1000}
  const lastFrame=Math.round((nativeClipEnd(clip)-nativeClipStart(clip))/1000000*rate.numerator/rate.denominator)
  if(!Number.isSafeInteger(startFrame)||!Number.isSafeInteger(endFrame)||Number(startFrame)<0||Number(endFrame)<=Number(startFrame)||Number(endFrame)>lastFrame||!Number.isSafeInteger(everyFrames)||Number(everyFrames)<1||typeof tolerancePx!=='number'||!Number.isFinite(tolerancePx)||tolerancePx<=0||xyMode!=='editable'&&xyMode!=='bake')throw new Error('Invalid conversion range or precision')
  const position:CompositionPositionAnimation=current??{mode:'xy',x:clip.transform.x,y:clip.transform.y}
  const request=packPositionConversion(position,{fpsNum:rate.numerator,fpsDen:rate.denominator,startFrame:Number(startFrame),endFrame:Number(endFrame),tolerancePx,everyFrames:xyMode==='bake'?Number(everyFrames):1,xyMode})
  const result=await (signal?previewPositionConversion(request,signal):previewPositionConversion(request))
  if(signal?.aborted)throw new Error('Motion path edit cancelled')
  if(!result.withinTolerance)throw new Error(`Conversion tolerance not achieved: ${result.maxErrorPx} px; ${result.limit??'no limit reported'}`)
  return nativePositionConversionCandidate(project,scope,clip.id,result,String(op.nodeIdPrefix))
 }
 if(op.op==='insert_path_node_near'){
  if(current?.mode!=='path')throw new Error('Motion path required')
  const point=op.point as Record<string,unknown>|null
  if(!isStableId(op.nodeId)||!point||typeof point!=='object'||Array.isArray(point)||Object.keys(point).some(key=>key!=='x'&&key!=='y')||typeof point.x!=='number'||typeof point.y!=='number'||!Number.isFinite(point.x)||!Number.isFinite(point.y)||typeof op.maxDistance!=='number'||!Number.isFinite(op.maxDistance)||op.maxDistance<0)throw new Error('Invalid path insertion point/tolerance')
  const response=await geometry({operation:'nearest',path:structuredClone(current.path),point:{x:point.x,y:point.y}})
  const hit=response.nearest
  if(!hit||!Number.isFinite(hit.distance)||hit.distance>op.maxDistance)throw new Error('No nearby path segment')
  return applyNativePathOperation(project,scope,{op:'edit_motion_path',clipId:clip.id,edit:{operation:'insert',segment:hit.segment,t:hit.t,id:op.nodeId}},signal)
 }
 if(op.op==='arrive_at_path_node'){
  if(current?.mode!=='path')throw new Error('Motion path required')
  if(!Number.isSafeInteger(op.index)||Number(op.index)<0||Number(op.index)>=current.path.nodes.length||!Number.isSafeInteger(op.playheadTicks)||Number(op.playheadTicks)<nativeClipStart(clip)||Number(op.playheadTicks)>nativeClipEnd(clip))throw new Error('Invalid path arrival node/time')
  const measured=await geometry({operation:'measure',path:structuredClone(current.path)})
  const progress=measured.nodeProgresses?.[Number(op.index)]
  if(measured.nodeProgresses?.length!==current.path.nodes.length||typeof progress!=='number'||!Number.isFinite(progress))throw new Error('Native path measure omitted node progress')
  return applyNativePathOperation(project,scope,{op:'set_path_progress',clipId:clip.id,playheadTicks:op.playheadTicks,value:progress},signal)
 }
 if(op.op==='edit_motion_path'){
  if(current?.mode!=='path')throw new Error('Motion path required')
  const edit=op.edit
  if(!edit||typeof edit!=='object'||Array.isArray(edit))throw new Error('Invalid motion path edit')
  const row=edit as Record<string,unknown>
  const fields:Record<string,readonly string[]>={translate:['delta'],remove:['index'],toggle_segment:['index'],solve:[],edit:['index','part','value'],mode:['index','mode'],insert:['segment','t','id'],append:['id','point']}
  if(typeof row.operation!=='string'||!Object.hasOwn(fields,row.operation)||Object.keys(row).some(field=>field!=='operation'&&!fields[row.operation as string]!.includes(field))||fields[row.operation]!.some(field=>row[field]===undefined))throw new Error('Invalid motion path edit')
  let request:NativeMotionPathRequest
  if(row.operation==='append'){
   const point=row.point as Record<string,unknown>|null
   if(!isStableId(row.id)||!point||typeof point!=='object'||Array.isArray(point)||Object.keys(point).some(key=>key!=='x'&&key!=='y')||typeof point.x!=='number'||typeof point.y!=='number'||!Number.isFinite(point.x)||!Number.isFinite(point.y))throw new Error('Invalid appended path node')
   request={operation:'solve',path:{nodes:[...structuredClone(current.path.nodes),{id:row.id as string,point:{x:point.x,y:point.y},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]}}
  }else request={...row,path:structuredClone(current.path)} as NativeMotionPathRequest
  const response=await geometry(request)
  const changed={...clip,transform:{...clip.transform,position:{...current,path:response.path}}}
  return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clip.id?changed:item)} as WireCompositionTrack:row)}}}}
 }
 if(op.op==='set_path_progress'){
  if(current?.mode!=='path')throw new Error('Motion path required')
  if(!Number.isSafeInteger(op.playheadTicks)||Number(op.playheadTicks)<nativeClipStart(clip)||Number(op.playheadTicks)>nativeClipEnd(clip)||typeof op.value!=='number'||!Number.isFinite(op.value))throw new Error('Invalid path progress time/value')
  const progress=current.progress,local=Number(op.playheadTicks)-nativeClipStart(clip)
  const tick=progress.mode==='keyframes'?keyframeAuthoredTick(progress.track,local):Math.round(local)
  if(!Number.isSafeInteger(tick))throw new Error('Path progress tick exceeds safe range')
  let candidate=project
  if(progress.mode==='constant')candidate=applyNativeKeyframeOperation(candidate,scope,{op:'upsert_keyframe',clipId:clip.id,envelope:'path',property:'progress',tick:0,value:progress.value})
  return applyNativeKeyframeOperation(candidate,scope,{op:'upsert_keyframe',clipId:clip.id,envelope:'path',property:'progress',tick,value:op.value})
 }
 if(op.op!=='switch_position_mode'||op.positionMode!=='xy'&&op.positionMode!=='path')throw new Error('Unknown position mode')
 if(!isStableId(op.nodeId))throw new Error('Invalid path node ID')
 if(current?.mode===op.positionMode)return project
 let position:CompositionPositionAnimation
 if(op.positionMode==='xy'){
  if(current?.mode!=='path'||current.path.nodes.length!==1)throw new Error('Multi-node path requires motion conversion')
  const point=current.path.nodes[0]!.point
  position={mode:'xy',x:{mode:'constant',value:point.x},y:{mode:'constant',value:point.y}}
 }else{
  const x=current?.mode==='xy'?current.x:clip.transform.x,y=current?.mode==='xy'?current.y:clip.transform.y
  if(x.mode!=='constant'||y.mode!=='constant')throw new Error('Animated XY requires motion conversion')
  position={mode:'path',path:{nodes:[{id:op.nodeId as string,point:{x:x.value,y:y.value},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]},progress:{mode:'constant',value:0}}
 }
 const changed={...clip,transform:{...clip.transform,position}}
 return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clip.id?changed:item)} as WireCompositionTrack:row)}}}}
}
