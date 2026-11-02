import { linkedClipIds } from './links'
import { findClipLocation, shiftClipAutomation } from './commands'
import { insertCompositionClips } from './globalInsert'
import { videoSourceTickAtTimelineTick } from './playback'
import { clipEndTicks, type Composition, type CompositionClip } from './types'
import { isSafeTick } from './validation'

/** Insert a silent held picture, retaining the moving head and tail. */
export function insertFreezeFrame(document: Composition, clipId: string, at: number, duration: number, allocate: () => string): { document: Composition; clipId: string; trackId: string } {
  const { track, clip } = findClipLocation(document, clipId)
  for (const id of linkedClipIds(document, clipId)) {
    const member = findClipLocation(document, id)
    if (member.track.locked || member.clip.locked) throw new Error('Клип или связанная дорожка заблокированы')
    if (member.track.id !== track.id && member.track.syncLock === false) throw new Error('Включите синхронный сдвиг связанной дорожки')
  }
  if (clip.kind !== 'video' && clip.kind !== 'image') throw new Error('Выберите видео или изображение')
  if (!isSafeTick(at) || at <= clip.timelineStartTicks || at >= clipEndTicks(clip)) throw new Error('Плейхед должен находиться внутри клипа')
  if (!isSafeTick(duration) || duration <= 0) throw new Error('Длительность стоп-кадра должна быть положительной')
  const sourceTick = clip.kind === 'video' ? videoSourceTickAtTimelineTick(clip, at) : 0
  const id = allocate()
  const picture = shiftClipAutomation(clip, clip.timelineStartTicks - at)
  const hold: CompositionClip = picture.kind === 'video' ? {
    ...picture, id, timelineStartTicks: at, timelineDurationTicks: duration, speed: 1,
    speedRamp: undefined, playbackMode: { mode: 'freeze', sourceTick },
    sourceAudioEnabled: false, audioAnimation: undefined,
  } : { ...picture, id, timelineStartTicks: at, durationTicks: duration }
  let result = insertCompositionClips(document, [{ trackId: track.id, clip: hold }], allocate)
  const pair = document.metadata?.mediaVersions?.[clipId]
  if (pair) result = { ...result, metadata: { ...result.metadata, mediaVersions: { ...result.metadata?.mediaVersions, [id]: { ...pair } } } }
  const name = document.metadata?.clipNames?.[clipId]
  if (name !== undefined) result = { ...result, metadata: { ...result.metadata, clipNames: { ...result.metadata?.clipNames, [id]: name } } }
  return { document: result, clipId: id, trackId: track.id }
}

export function insertFreezeReason(document: Composition, clipId: string | null, at: number): string | null {
  if (!clipId) return 'Выберите видео или изображение'
  const reserved = new Set([...document.tracks.flatMap(t => [t.id, ...t.clips.map(c => c.id)]), ...(document.links ?? []).map(l => l.id), ...(document.captionTracks ?? []).flatMap(t => [t.id, ...t.clips.map(c => c.id)])])
  let n = 0
  const allocate = () => { let id: string; do { id = `freeze-check-${n++}` } while (reserved.has(id)); reserved.add(id); return id }
  try { insertFreezeFrame(document, clipId, at, 1_000_000, allocate); return null }
  catch (error) { return error instanceof Error ? error.message : 'Не удалось вставить стоп-кадр' }
}
