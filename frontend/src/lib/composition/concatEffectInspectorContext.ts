import type {NativeNestedProject} from '$lib/api'
import type {WireCompositionTrack,WireVideoClip,WireImageClip,WireTextClip,WireGraphicClip} from './types'
type VisualTrack=Extract<WireCompositionTrack,{kind:'video'|'image'|'text'|'graphic'}>
/** Keep ownership explicit: ordinary text/graphic engines use their own inspectors. */
export function concatEffectInspectorTracks(project:NativeNestedProject|null,compositionId:string):readonly VisualTrack[]{
 const tracks:VisualTrack[]=[],node=project&&Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
 for(const track of node?.content.tracks??[]){
  if(track.kind==='video'||track.kind==='image')tracks.push(track)
  else if(track.kind==='text')tracks.push({...track,clips:track.clips.filter(clip=>clip.concatTitle)})
  else if(track.kind==='graphic')tracks.push({...track,clips:track.clips.filter(clip=>clip.concatShape||clip.concatTreatment)})
 }
 return tracks
}
/** Treatments use absolute generator time; retained graphic pictures include their local trim offset. */
export function concatEffectNativeKeyOrigin(clip:WireVideoClip|WireImageClip|WireTextClip|WireGraphicClip):number{
 if('concatTreatment' in clip&&clip.concatTreatment)return 0
 if('placement' in clip)return clip.placement.timelineStartTick
 return clip.timelineStartTick-('animationOffsetUs' in clip?(clip.animationOffsetUs??0):0)
}
