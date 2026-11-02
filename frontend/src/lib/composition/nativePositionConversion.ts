import type { NativeNestedProject, NativePositionConversionResult } from '$lib/api'
import type { WireCompositionTrack } from './types'
import { unpackPositionConversion } from './positionConversion'
import { isStableId } from './validation'
/** The preview and commit use the same detached native conversion result. */
export function nativePositionConversionCandidate(project:NativeNestedProject,scope:string,clipId:string,result:NativePositionConversionResult,prefix:string):NativeNestedProject {
  if(!isStableId(prefix))throw new Error('Invalid conversion node ID prefix')
  const node=project.compositions[scope],track=node?.content.tracks.find(row=>row.clips.some(clip=>clip.id===clipId)),clip=track?.clips.find(clip=>clip.id===clipId)
  if(!node||!track||!clip||!('transform' in clip))throw new Error('Visual layer required')
  if(track.locked)throw new Error('Track is locked')
  const position=unpackPositionConversion(result,index=>{const id=`${prefix}-${index}`;if(!isStableId(id))throw new Error('Invalid generated path node ID');return id})
  const changed={...clip,transform:{...clip.transform,position}}
  return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clipId?changed:item)} as WireCompositionTrack:row)}}}}
}
