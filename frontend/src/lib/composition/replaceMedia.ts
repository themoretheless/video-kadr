import { findClipLocation } from './commands'
import { assertCompositionLocksPreserved } from './locks'
import type { Composition, CompositionClip, VideoClip } from './types'
import { assertValidComposition, isSafeTick } from './validation'

/** Replace this placement only; other uses of the old source are retained. */
export function replaceClipMedia(document: Composition, clipId: string, sourceId: string, sourceStart?: number, processed = false): Composition {
  assertValidComposition(document)
  const { track, clip } = findClipLocation(document, clipId)
  if (track.locked || clip.locked) throw new Error('Клип или дорожка заблокированы')
  if (clip.kind !== 'video' && clip.kind !== 'audio' && clip.kind !== 'image') throw new Error('У этого клипа нет исходного медиа')
  const source = document.sources[sourceId]
  if (!source) throw new Error('Источник не зарегистрирован в проекте')
  if (clip.kind === 'video' && source.kind !== 'video' || clip.kind === 'image' && source.kind !== 'image' || clip.kind === 'audio' && (source.kind === 'image' || !source.hasAudio)) throw new Error('Тип нового источника несовместим с клипом')
  if (clip.kind === 'video' && clip.sourceAudioEnabled && clip.speedRamp?.audioPolicy !== 'mute' && !source.hasAudio) throw new Error('Клип использует звук: выберите источник с аудиодорожкой')
  let replacement: CompositionClip = { ...clip, sourceId }
  if (clip.kind !== 'image') {
    const start = sourceStart ?? clip.sourceInTicks
    if (!isSafeTick(start)) throw new Error('Некорректное начало исходного диапазона')
    const end = start + clip.sourceOutTicks - clip.sourceInTicks
    if (!isSafeTick(end) || end > source.durationTicks) throw new Error('Новый источник короче используемого диапазона')
    replacement = { ...clip, sourceId, sourceInTicks: start, sourceOutTicks: end,
      ...(clip.kind === 'video' && clip.playbackMode?.mode === 'freeze' ? { playbackMode: { ...clip.playbackMode, sourceTick: clip.playbackMode.sourceTick + start - clip.sourceInTicks } } : {}),
    }
  } else if (sourceStart !== undefined) throw new Error('Для изображения исходное время не задаётся')
  if(clip.kind==='video'&&clip.sourceId!==sourceId){const {concatCutout:_cutout,...unbound}=replacement as VideoClip;replacement=unbound}
  const mediaVersions = { ...document.metadata?.mediaVersions }
  if (processed) mediaVersions[clipId] = { originalSourceId: mediaVersions[clipId]?.originalSourceId ?? clip.sourceId, copySourceId: sourceId }
  else delete mediaVersions[clipId]
  const result = { ...document, ...(processed || document.metadata?.mediaVersions ? { metadata: { ...document.metadata, mediaVersions } } : {}), tracks: document.tracks.map(t => t.id === track.id ? { ...t, clips: t.clips.map(c => c.id === clipId ? replacement : c) } as typeof t : t) }
  assertValidComposition(result)
  assertCompositionLocksPreserved(document, result)
  return result
}
export function replaceClipMediaReason(document: Composition, clipId: string, sourceId: string): string | null {
  try { replaceClipMedia(document, clipId, sourceId); return null }
  catch (error) { return error instanceof Error ? error.message : 'Не удалось заменить медиа' }
}

/** Keep authored placement/looks while choosing one of the stored media versions. */
export function setClipMediaVersion(document: Composition, clipId: string, copy: boolean): Composition {
  const versions = document.metadata?.mediaVersions?.[clipId]
  if (!versions) throw new Error('У клипа нет сохранённой обработанной копии')
  const next = replaceClipMedia(document, clipId, copy ? versions.copySourceId : versions.originalSourceId)
  return { ...next, metadata: { ...next.metadata, mediaVersions: { ...next.metadata?.mediaVersions, [clipId]: { ...versions } } } }
}
