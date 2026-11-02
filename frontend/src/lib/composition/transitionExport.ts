import type {Composition,CompositionTransitionKind,VideoTrack} from './types'

const nativeKinds=new Set<CompositionTransitionKind>(['concat_dissolve','concat_fade_black','concat_fade_white','dissolve','film_dissolve','additive_dissolve','fade_black','fade_white','wipe_left','wipe_right','wipe_up','wipe_down','procedural'])
export function independentTransitionExportReason(c:Composition):string|null {
 return independentTransitionReason(c,false)
}

/** Visual frame eligibility; audio routing is qualified separately by export. */
export function independentTransitionPreviewReason(c:Composition):string|null {
 return independentTransitionReason(c,true)
}

function independentTransitionReason(c:Composition,preview:boolean):string|null {
 const video=c.tracks.filter((track):track is VideoTrack=>track.kind==='video'&&!track.hidden)
 if(!video.some(track=>track.transitions?.some(t=>t.timeline)))return null
 const primary=c.tracks.findLast(track=>track.kind!=='audio'&&!track.hidden&&(track.clips.some(clip=>clip.enabled!==false)||(track.kind==='video'&&track.transitions?.some(t=>t.timeline))))
 for(const track of video){
  if(!track.transitions?.some(t=>t.timeline))continue
  if(track.transitions.some(t=>!t.timeline))return 'Совмещение старых и независимых переходов ещё не поддерживается.'
  if(!preview&&track!==primary&&!track.muted&&track.clips.some(clip=>clip.sourceAudioEnabled))return 'Звук независимых переходов на overlay ещё не подключён.'
  for(const transition of track.transitions){
   if(!nativeKinds.has(transition.kind)&&!(preview&&transition.kind==='procedural'))return 'Этот независимый переход ещё не перенесён в нативный рендер.'
   for(const id of [transition.fromClipId,transition.toClipId]){
    const clip=track.clips.find(clip=>clip.id===id)
    if(!clip||clip.enabled===false)continue
    if(clip.videoEffects?.length||clip.chromaKey?.enabled||clip.masks?.length)return 'Эффекты сторон независимого перехода требуют точного времени за границами клипа.'
    if(clip.speedRamp)return 'Speed ramp независимого перехода ещё не подключён.'
    if((clip.stabilization?.mode??'disabled')!=='disabled'||(clip.frameInterpolation??'duplicate')!=='duplicate')return 'Стабилизация и optical flow независимого перехода ещё не проверены.'
   }
  }
 }
 return null
}
