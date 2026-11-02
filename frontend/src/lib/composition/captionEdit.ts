import type {Composition,WireComposition} from './types'
import {normalizeComposition} from './validation'
/** Preserve editor-only data while accepting an atomic native caption transaction. */
export function applyCaptionEdit(before:Composition,after:WireComposition):Composition {
 const tracks=before.captionTracks??[],changed=after.captionTracks??[]
 if(tracks.length!==changed.length || tracks.some((track,index)=>track.id!==changed[index].id))throw new Error('Изменился список дорожек субтитров')
 if(before.tracks.length!==after.tracks.length || before.tracks.some((track,index)=>track.id!==after.tracks[index].id||track.kind!==after.tracks[index].kind))throw new Error('Изменилась структура композиции')
 return normalizeComposition({...before,captionTracks:tracks.map((track,index)=>({...track,clips:changed[index].clips.map(cue=>{
  const original=track.clips.find(original=>original.id===cue.id)
  if(original&&original.timelineStartTick===cue.timelineStartTick&&original.timelineEndTick===cue.timelineEndTick&&original.text===cue.text&&(original.speaker??null)===(cue.speaker??null)&&(original.settings??'')===(cue.settings??''))return original
  return {...original,...JSON.parse(JSON.stringify(cue))}
 })}))})
}
