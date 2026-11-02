import {moveNativeNestedClips,type NativeNestedProject,type NativeClipMove} from '../api'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
import {removeNativeClips} from './nativeClipDelete'
import {expandNativeClipLinks} from './nativeClipLinks'
import {rebindNativeLayerReferences} from './nativeLayerReferences'
import {extractCaptionRanges,type CompositionGap} from './timelineProductivity'

export function nativeGapAt(track:import('./types').WireCompositionTrack,tick:number):CompositionGap|null{
 if(!Number.isSafeInteger(tick)||tick<0)return null
 let start=0,end=Infinity
 for(const clip of track.clips){const a=nativeClipStart(clip),b=nativeClipEnd(clip);if(a<=tick&&tick<b)return null;if(b<=tick)start=Math.max(start,b);else end=Math.min(end,a)}
 return Number.isFinite(end)?{start,end}:null
}

/** Plan in original clocks; native move admission validates the final retained clips. */
export async function rippleNativeComposition(project:NativeNestedProject,scope:string,request:{clipIds:readonly string[]}|{trackId:string;start:number;end:number}):Promise<NativeNestedProject>{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw new Error('Composition not found')
 const all=node.content.tracks.flatMap(track=>track.clips.map(clip=>({track,clip,start:nativeClipStart(clip),end:nativeClipEnd(clip)})))
 const doomed=new Set('clipIds' in request?expandNativeClipLinks(project,scope,request.clipIds):[])
 if(doomed.size>512)throw new Error('Ripple delete accepts at most 512 linked clips')
 const remaining=all.filter(row=>!doomed.has(row.clip.id))
 let raw:CompositionGap[]=[]
 if('clipIds' in request){
  if(!doomed.size||all.filter(row=>doomed.has(row.clip.id)).length!==doomed.size)throw new Error('Ripple layers not found')
  for(const row of all.filter(row=>doomed.has(row.clip.id))){
   if(row.track.locked)throw new Error('Track is locked')
   const lane=remaining.filter(other=>other.track.id===row.track.id)
   const start=Math.max(row.start,...lane.filter(other=>other.start<row.start).map(other=>other.end))
   const end=Math.min(row.end,...lane.filter(other=>other.start>=row.start).map(other=>other.start))
   if(end>start)raw.push({start,end})
  }
 }else{
  const track=node.content.tracks.find(track=>track.id===request.trackId)
  if(!track)throw new Error('Track not found')
  const gap=nativeGapAt(track,request.start)
  if(!Number.isSafeInteger(request.end)||!gap||gap.start!==request.start||gap.end!==request.end||request.end<=request.start)throw new Error('Gap no longer exists at the exact supplied boundaries')
  raw=[gap]
 }
 raw.sort((a,b)=>a.start-b.start)
 const holes:CompositionGap[]=[]
 for(const hole of raw){const last=holes.at(-1);if(last&&hole.start<=last.end)holes[holes.length-1]={start:last.start,end:Math.max(last.end,hole.end)};else holes.push({...hole})}
 const instances=node.instances.map(instance=>({id:instance.id,start:instance.timing.timelineStartUs}))
 for(const hole of holes)for(const row of [...remaining.map(row=>({id:row.clip.id,start:row.start})),...instances])if(row.start>=hole.start&&row.start<hole.end&&!(hole.start===0&&row.start===0))throw new Error(`Ripple: ${row.id} starts inside the deleted interval`)
 const delta=(start:number)=>holes.reduce((sum,hole)=>sum+(hole.end<=start?hole.end-hole.start:0),0)
 const deltas=new Map(remaining.map(row=>[row.clip.id,delta(row.start)]))
 for(const members of Object.values(node.content.metadata?.clipLinks??{})){
  const retained=members.filter(id=>!doomed.has(id)),shifts=new Set(retained.map(id=>deltas.get(id)??0))
  if(shifts.size>1)throw new Error('Ripple would split a linked group across the deleted interval')
 }
 let candidate=doomed.size?removeNativeClips(project,scope,[...doomed]):project
 const current=candidate.compositions[scope]!
 for(const track of current.content.tracks)if(track.kind==='video')for(const transition of track.transitions)if(transition.fromClipId&&transition.toClipId&&deltas.get(transition.fromClipId)!==deltas.get(transition.toClipId))throw new Error('Ripple would change a transition overlap')
 const moves:NativeClipMove[]=remaining.filter(row=>(deltas.get(row.clip.id)??0)>0).map(row=>{
  if(row.track.locked)throw new Error('Track is locked')
  return {fromComposition:scope,fromTrackId:row.track.id,clipId:row.clip.id,toComposition:scope,toTrackId:row.track.id,startUs:row.start-deltas.get(row.clip.id)!,createTrack:false}
 })
 if(moves.length){const result=await moveNativeNestedClips(candidate,moves);candidate={...candidate,compositions:{...candidate.compositions,[scope]:result.project.compositions[scope]!}}}
 const after=candidate.compositions[scope]!
 return {...candidate,compositions:{...candidate.compositions,[scope]:{...after,instances:after.instances.map(instance=>{const shift=delta(instance.timing.timelineStartUs);return shift?{...instance,timing:{...instance.timing,timelineStartUs:instance.timing.timelineStartUs-shift}}:instance}),content:{...after.content,tracks:rebindNativeLayerReferences(node.content.tracks,after.content.tracks),...(after.content.captionTracks?{captionTracks:extractCaptionRanges(after.content.captionTracks,holes)}:{})}}}}
}
