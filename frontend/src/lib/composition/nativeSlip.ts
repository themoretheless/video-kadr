import type {NativeNestedProject} from '../api'
import type {WireCompositionTrack} from './types'
export function slipNativeLayer(project:NativeNestedProject,scope:string,clipId:string,delta:number):NativeNestedProject{
 if(!Number.isSafeInteger(delta))throw new Error('Slip delta must be a safe signed source tick')
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 const track=node?.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId)),clip=track?.clips.find(clip=>clip.id===clipId)
 if(!node||!track||!clip||!('placement' in clip))throw new Error('Slip requires a video or audio layer')
 if(track.locked)throw new Error('Track is locked')
 const placement=clip.placement
 if(placement.speedRamp||'playbackMode' in clip&&clip.playbackMode.mode==='freeze')throw new Error('Freeze and speed-ramped layers cannot slip')
 const source=node.content.sources[clip.sourceId]
 if(!source)throw new Error('Slip source not found')
 const reverse='playbackMode' in clip?clip.playbackMode.mode==='reverse':clip.reversed
 const span=placement.sourceOutTick-placement.sourceInTick,phase=placement.sourceTimelineOffsetTick??0
 const head=Math.ceil(Math.max(placement.sourceHeadTimelineTick??0,-phase,0)*placement.speed)
 if(!Number.isSafeInteger(head)||head<0||!Number.isSafeInteger(source.durationTicks)||span<=0)throw new Error('Invalid retained source range')
 const minimum=reverse?0:head,maximum=source.durationTicks-span-(reverse?head:0)
 if(maximum<minimum)throw new Error('Not enough retained source material')
 const start=Math.max(minimum,Math.min(maximum,placement.sourceInTick+delta))
 if(start===placement.sourceInTick)return project
 const changed={...clip,placement:{...placement,sourceInTick:start,sourceOutTick:start+span}}
 return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:node.content.tracks.map(row=>row.id===track.id?{...row,clips:row.clips.map(item=>item.id===clipId?changed:item)} as WireCompositionTrack:row)}}}}
}
