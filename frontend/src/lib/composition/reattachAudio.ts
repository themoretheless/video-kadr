import {cloneDspEffect} from './dsp'
import {audioRoleGain} from './audioRoles'
import {cloneGainSourceKeys} from './concatGainSourceKeys'
import {hasConcatDetachedAudio,reattachConcatAudio} from './concatDetachedAudio'
import { cloneAudioAnimation } from './keyframes.js'
import { deleteClip, findClipLocation } from './commands.js'
import { linkedClipIds } from './links.js'
import { assertCompositionLocksPreserved } from './locks.js'
import { clipEndTicks, type AudioClip, type Composition, type CompositionTrack, type VideoClip } from './types.js'
import { assertValidComposition, primaryCompositionVideoTrack } from './validation.js'

/** Rejoin a linked source-audio pair without guessing from shared media IDs. */
export function reattachAudio(document: Composition, clipId: string): { document: Composition; videoId: string } {
  if(hasConcatDetachedAudio(document,clipId))return reattachConcatAudio(document,clipId)
  assertValidComposition(document)
  findClipLocation(document, clipId)
  const members = linkedClipIds(document, clipId).map(id => findClipLocation(document, id))
  const videos = members.filter((m): m is typeof m & { clip: VideoClip } => m.clip.kind === 'video')
  if (videos.length !== 1) throw new Error('Выберите связанную пару видео и звука')
  const picture = videos[0]!
  const video = picture.clip
  const sounds = members.filter((m): m is typeof m & { clip: AudioClip } => m.clip.kind === 'audio' && m.clip.sourceId === video.sourceId)
  if (sounds.length !== 1) throw new Error('Для возврата нужна одна связанная звуковая дорожка исходника')
  const sound = sounds[0]!
  const audio = sound.clip
  if (picture.track.locked || sound.track.locked || video.locked || audio.locked) throw new Error('Видео или звук заблокированы')
  if (video.sourceAudioEnabled) throw new Error('Встроенный звук уже включён')
  if (picture.track.kind !== 'video' || picture.track.muted || video.enabled === false || primaryCompositionVideoTrack(document)?.id !== picture.track.id) throw new Error('Вернуть встроенный звук можно только в активное основное видео')
  if (!document.sources[video.sourceId]?.hasAudio || video.playbackMode?.mode === 'freeze') throw new Error('У видео нет непрерывного встроенного звука')
  if (audio.timelineStartTicks !== video.timelineStartTicks || clipEndTicks(audio) !== clipEndTicks(video) ||
      audio.sourceInTicks !== video.sourceInTicks || audio.sourceOutTicks !== video.sourceOutTicks || audio.speed !== video.speed ||
      !!audio.reversed !== (video.playbackMode?.mode === 'reverse') || JSON.stringify(audio.speedRamp ?? null) !== JSON.stringify(video.speedRamp ?? null)) {
    throw new Error('Звук рассинхронизирован: сначала совместите границы и скорость с видео')
  }
  if (audio.enabled === false || sound.track.kind !== 'audio' || sound.track.muted || sound.track.solo ||
      audio.crossfadeInTicks || audio.ducking ||
      (audio.voiceEffect && audio.voiceEffect !== 'none') || audio.pitchSemitones || audio.toneDb ||
      (audio.role && audio.role !== 'dialogue')) throw new Error('Обработка отдельного звука не поддерживается встроенной дорожкой')
  if(audioRoleGain(document,audio.role??'dialogue')!==1)throw new Error('Сначала сбросьте микс аудиороли: встроенный звук не проходит через него')
  let next = deleteClip(document, audio.id)
  next = { ...next, tracks: next.tracks.map(track => ({ ...track, clips: track.clips.map(clip => clip.id !== video.id ? clip : {
    ...video, audioStream:audio.audioStream, sourceAudioEnabled: true, audioGain: audio.gain, audioPan: audio.pan ?? 0,
    audioAnimation: cloneAudioAnimation(audio.audioAnimation),
    concatGainKeys:cloneGainSourceKeys(audio.concatGainKeys??[]),
    dspEffects:audio.dspEffects?.map(cloneDspEffect)??[],
    audioFadeInTicks:audio.fadeInTicks??0,audioFadeOutTicks:audio.fadeOutTicks??0,
  }) } as CompositionTrack)) }
  assertValidComposition(next)
  assertCompositionLocksPreserved(document, next)
  return { document: next, videoId: video.id }
}

export function reattachAudioReason(document: Composition, clipId: string): string | null {
  try { reattachAudio(document, clipId); return null } catch (error) { return error instanceof Error ? error.message : 'Не удалось вернуть звук' }
}
