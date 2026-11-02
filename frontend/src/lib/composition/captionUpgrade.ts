import {compareCaptionTimes} from './captionTime'
import type {Composition, GraphicClip, WireComposition} from './types'
import {createGraphicClip,cloneGraphicValue} from './graphicLayers'
import {normalizeComposition} from './validation'

/** Retain editor-only data and map the native transaction's new graphics into editor clips. */
export function applyCaptionUpgrade(before:Composition,after:WireComposition):Composition {
  const [added,...existing]=after.tracks
  if (!added || added.kind!=='graphic' || existing.length!==before.tracks.length || existing.some((track,i)=>track.id!==before.tracks[i].id || track.kind!==before.tracks[i].kind)) throw new Error('Изменилась структура композиции')
  const remaining=new Set((after.captionTracks??[]).flatMap(track=>track.clips.map(cue=>cue.id)))
  const removed=(before.captionTracks??[]).flatMap(track=>track.clips.filter(cue=>!remaining.has(cue.id)))
  if (removed.length!==added.clips.length || !removed.length) throw new Error('Неверный результат преобразования субтитров')
  const clips:GraphicClip[]=added.clips.map((clip,i)=>{
    const cue=removed[i]
    if (compareCaptionTimes(clip.timelineStartTick,cue.timelineStartTick)!==0 || compareCaptionTimes(clip.timelineEndTick,cue.timelineEndTick)!==0) throw new Error('Изменилось время субтитров')
    return {...createGraphicClip(before.canvas,clip.id,clip.timelineStartTick,clip.timelineEndTick-clip.timelineStartTick,'text'),graphic:cloneGraphicValue(clip.graphic),width:clip.width,height:clip.height,effects:cloneGraphicValue(clip.effects),enabled:clip.enabled}
  })
  return normalizeComposition({...before,metadata:{...before.metadata,clipNames:{...before.metadata?.clipNames,...Object.fromEntries(clips.map(clip=>[clip.id,after.metadata?.clipNames?.[clip.id]??'Caption']))}},captionTracks:before.captionTracks?.map(track=>({...track,clips:track.clips.filter(cue=>remaining.has(cue.id))})),tracks:[{kind:'graphic',id:added.id,name:added.name,hidden:added.hidden,locked:added.locked,clips},...before.tracks]})
}
