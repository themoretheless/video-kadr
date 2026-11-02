import type {NativeScalarChannel,NativeScalarKeySelection} from './nativeScalarTimeline'
import {keyframeLocalTime} from './keyframeClock'
export interface ScalarTimeView {start:number;end:number}
export type ScalarSelectionMode='replace'|'add'|'toggle'|'subtract'
function validView(view:ScalarTimeView){if(!Number.isFinite(view.start)||!Number.isFinite(view.end)||view.end<=view.start)throw new Error('Некорректная видимая область времени')}
export function nativeScalarExtent(channels:readonly NativeScalarChannel[],spans:readonly ScalarTimeView[]=[]):ScalarTimeView{
 let start=0,end=1000000
 for(const span of spans){start=Math.min(start,span.start);end=Math.max(end,span.end)}
 for(const channel of channels){start=Math.min(start,channel.start);end=Math.max(end,channel.start+channel.duration);for(const key of channel.value.track.keyframes){const time=channel.start+keyframeLocalTime(channel.value.track,key.tick);start=Math.min(start,time);end=Math.max(end,time)}}
 return {start:Math.floor(start),end:Math.ceil(Math.max(start+1000000,end))}
}
export function constrainScalarTimeView(view:ScalarTimeView,extent:ScalarTimeView):ScalarTimeView{
 validView(view);validView(extent)
 const span=Math.min(extent.end-extent.start,Math.max(1,view.end-view.start)),start=Math.max(extent.start,Math.min(extent.end-span,view.start))
 return {start,end:start+span}
}
export function zoomScalarTimeView(view:ScalarTimeView,extent:ScalarTimeView,factor:number,anchor:number):ScalarTimeView{
 if(!Number.isFinite(factor)||factor<=0||!Number.isFinite(anchor))throw new Error('Некорректный масштаб области времени')
 const current=constrainScalarTimeView(view,extent),span=current.end-current.start,at=Math.max(extent.start,Math.min(extent.end,anchor)),fraction=Math.max(0,Math.min(1,(at-current.start)/span)),next=Math.min(extent.end-extent.start,Math.max(1,span/factor))
 return constrainScalarTimeView({start:at-fraction*next,end:at+(1-fraction)*next},extent)
}
export function panScalarTimeView(view:ScalarTimeView,extent:ScalarTimeView,delta:number):ScalarTimeView{
 if(!Number.isFinite(delta))throw new Error('Некорректный сдвиг области времени')
 return constrainScalarTimeView({start:view.start+delta,end:view.end+delta},extent)
}
export function combineScalarKeySelection(previous:readonly NativeScalarKeySelection[],hits:readonly NativeScalarKeySelection[],mode:ScalarSelectionMode):NativeScalarKeySelection[]{
 if(!['replace','add','toggle','subtract'].includes(mode))throw new Error('Некорректный режим выделения')
 const id=(key:NativeScalarKeySelection)=>JSON.stringify([key.channelId,key.tick]),map=new Map(previous.map(key=>[id(key),{...key}]))
 if(mode==='replace')map.clear()
 for(const key of hits){const address=id(key);if(mode==='subtract'||mode==='toggle'&&map.has(address))map.delete(address);else map.set(address,{...key})}
 return [...map.values()]
}
export function scalarKeysInTimeRange(channels:readonly NativeScalarChannel[],from:number,to:number):NativeScalarKeySelection[]{
 if(!Number.isFinite(from)||!Number.isFinite(to))throw new Error('Некорректный диапазон выделения')
 const low=Math.min(from,to),high=Math.max(from,to)
 return channels.filter(channel=>!channel.locked).flatMap(channel=>channel.value.track.keyframes.filter(key=>{const time=channel.start+keyframeLocalTime(channel.value.track,key.tick);return time>=low&&time<=high}).map(key=>({channelId:channel.id,tick:key.tick})))
}
export interface ScalarScreenKey {key:NativeScalarKeySelection;x:number;y:number}
export function scalarKeysInBox(points:readonly ScalarScreenKey[],x1:number,y1:number,x2:number,y2:number):NativeScalarKeySelection[]{
 if(![x1,y1,x2,y2].every(Number.isFinite))throw new Error('Некорректная рамка выделения')
 const left=Math.min(x1,x2),right=Math.max(x1,x2),top=Math.min(y1,y2),bottom=Math.max(y1,y2)
 return points.filter(point=>point.x>=left&&point.x<=right&&point.y>=top&&point.y<=bottom).map(point=>({...point.key}))
}

/** Bounded edge-pan speed in viewport spans per second. */
export function scalarEdgePanSpeed(clientX:number,left:number,width:number):number{
 if(![clientX,left,width].every(Number.isFinite)||width<=0)return 0
 const edge=Math.min(32,width/4),position=clientX-left
 if(position<edge)return -Math.min(1,(edge-position)/edge)
 if(position>width-edge)return Math.min(1,(position-width+edge)/edge)
 return 0
}
