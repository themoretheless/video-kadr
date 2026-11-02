import type { NativeNestedProject } from '$lib/api'
import { MAX_COMPOSITION_CLIPS, MAX_COMPOSITION_DURATION_TICKS, MAX_COMPOSITION_TRACKS, type CaptionTrack } from './types'
import { exactCaptionTicks, captionTimestampFromExact, CAPTION_SOURCE_TICKS_PER_MICROSECOND as UNIT } from './captionTime'
import { isStableId } from './validation'
export function captureNativeCaptionTracks(project: NativeNestedProject, from: string, ids: readonly string[]): readonly CaptionTrack[] {
  const node=project.compositions[from]
  if (!node) throw new Error('Таймлайн не найден')
  const selected=new Set(ids), found=new Set<string>()
  const tracks=(node.content.captionTracks ?? []).filter(track=>track.clips.some(cue=>selected.has(cue.id))).map(track=>{
    if (track.locked) throw new Error('Дорожка субтитров заблокирована')
    const clips=track.clips.filter(cue=>selected.has(cue.id));clips.forEach(cue=>found.add(cue.id))
    return {...track,clips}
  })
  if (found.size!==selected.size) throw new Error('Выбранный субтитр не найден')
  return JSON.parse(JSON.stringify(tracks)) as CaptionTrack[]
}
function trackSignature(track: CaptionTrack): string {
  const { id: _id, clips: _clips, name: _name, locked: _locked, ...settings }=track
  return JSON.stringify(settings)
}
export function pasteNativeCaptionTracks(project: NativeNestedProject, to: string, captured: readonly CaptionTrack[], deltaUs: number, targets: Readonly<Record<string,string>>, allocate: () => string): NativeNestedProject {
  const node=project.compositions[to]
  if (!node) throw new Error('Таймлайн не найден')
  if (!captured.length) return project
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректное время вставки субтитров')
  const reserved=new Set([...node.content.tracks.map(track=>track.id),...(node.content.captionTracks ?? []).map(track=>track.id),...node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)),...(node.content.captionTracks ?? []).flatMap(track=>track.clips.map(cue=>cue.id)),...node.content.tracks.flatMap(track=>track.kind==='video'?track.transitions.map(transition=>transition.id):[])])
  const mint=()=>{const id=allocate();if (!isStableId(id) || reserved.has(id)) throw new Error('Нужен уникальный идентификатор субтитра');reserved.add(id);return id}
  const tracks=[...(node.content.captionTracks ?? [])], delta=BigInt(deltaUs)*UNIT
  let end=0n
  for (const source of captured) {
    const targetId=targets[source.id] ?? '', index=tracks.findIndex(track=>track.id===targetId)
    if (targetId && index<0) throw new Error('Целевая дорожка субтитров не найдена')
    const target=index<0?undefined:tracks[index]
    if (target?.locked) throw new Error('Целевая дорожка субтитров заблокирована')
    if (target && trackSignature(target)!==trackSignature(source)) throw new Error('Оформление целевой дорожки отличается от буфера')
    const clips=source.clips.map(cue=>{
      const start=exactCaptionTicks(cue.timelineStartTick)+delta, stop=exactCaptionTicks(cue.timelineEndTick)+delta
      if (start<0n || stop<=start || stop>BigInt(MAX_COMPOSITION_DURATION_TICKS)*UNIT) throw new Error('Время субтитра выходит за допустимый диапазон')
      if (stop>end) end=stop
      return {...cue,id:mint(),timelineStartTick:captionTimestampFromExact(start,cue.timelineStartTick),timelineEndTick:captionTimestampFromExact(stop,cue.timelineEndTick)}
    })
    const merged=[...(target?.clips ?? []),...clips].sort((a,b)=>exactCaptionTicks(a.timelineStartTick)<exactCaptionTicks(b.timelineStartTick)?-1:1)
    if (merged.some((cue,i)=>i>0 && exactCaptionTicks(merged[i-1]!.timelineEndTick)>exactCaptionTicks(cue.timelineStartTick))) throw new Error('Субтитры перекрываются на целевой дорожке')
    if (target) tracks[index]={...target,clips:merged}
    else tracks.push({...source,id:mint(),clips:merged})
  }
  if (tracks.length+node.content.tracks.length>MAX_COMPOSITION_TRACKS) throw new Error('Слишком много дорожек')
  if (tracks.reduce((n,track)=>n+track.clips.length,0)+node.content.tracks.reduce((n,track)=>n+track.clips.length,0)>MAX_COMPOSITION_CLIPS) throw new Error('Слишком много клипов')
  const duration=Number((end+UNIT-1n)/UNIT)
  return {...project,compositions:{...project.compositions,[to]:{...node,...(node.durationUs!==undefined?{durationUs:Math.max(node.durationUs,duration)}:{}),content:{...node.content,captionTracks:tracks}}}}
}
