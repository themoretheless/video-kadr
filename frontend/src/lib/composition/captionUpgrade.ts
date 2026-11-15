import {compareCaptionTimes} from './captionTime'
import type {CaptionTrack, Composition, GraphicClip, WireComposition} from './types'
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

/** Apply only the graphics and removals from Rust to the existing nested wire document. */
export function applyNestedCaptionUpgrade(before: WireComposition, after: WireComposition, ids: readonly string[]): WireComposition {
  const [added, ...existing] = after.tracks
  if (!added || added.kind !== 'graphic' || existing.length !== before.tracks.length ||
      existing.some((track, index) => track.id !== before.tracks[index]?.id || track.kind !== before.tracks[index]?.kind)) {
    throw new Error('Изменилась структура композиции')
  }
  const selected = new Set(ids)
  const tracks = before.captionTracks ?? [], updated = after.captionTracks ?? []
  const removed = tracks.flatMap(track => track.clips.filter(cue => selected.has(cue.id)))
  if (!removed.length || removed.length !== selected.size || removed.length !== added.clips.length || tracks.length !== updated.length) {
    throw new Error('Неверный результат преобразования субтитров')
  }
  const captionTracks: CaptionTrack[] = tracks.map((track, index) => {
    const clips = track.clips.filter(cue => !selected.has(cue.id))
    const result = updated[index]
    if (result?.id !== track.id || result.clips.length !== clips.length || result.clips.some((cue, i) => cue.id !== clips[i]?.id)) {
      throw new Error('Изменился список реплик')
    }
    return { ...track, clips }
  })
  const clipNames = { ...before.metadata?.clipNames }
  for (const [index, clip] of added.clips.entries()) {
    const cue = removed[index]!
    if (compareCaptionTimes(clip.timelineStartTick, cue.timelineStartTick) !== 0 ||
        compareCaptionTimes(clip.timelineEndTick, cue.timelineEndTick) !== 0) {
      throw new Error('Изменилось время субтитров')
    }
    clipNames[clip.id] = after.metadata?.clipNames?.[clip.id] ?? 'Caption'
  }
  return { ...before, captionTracks, tracks: [added, ...before.tracks], metadata: { ...before.metadata, clipNames } }
}
