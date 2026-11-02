import {embeddedAudioMix} from './embeddedAudioMix'
import { addClip, addTrack, findClipLocation } from './commands'
import { createCompositionLink } from './links'
import { assertCompositionLocksPreserved } from './locks'
import { assertValidComposition } from './validation'
import type { AudioClip, Composition, CompositionTrack } from './types'

/** Atomic V+A import: same source/span/phase, one audible audio owner. */
export function pairImportedVideoAudio(document: Composition, clipId: string, ids: { audioTrackId: string; audioClipId: string; linkId: string }): Composition {
  assertValidComposition(document)
  const { clip, track } = findClipLocation(document, clipId)
  if (clip.kind !== 'video' || !document.sources[clip.sourceId]?.hasAudio) throw new Error('Pairing requires video with an audio stream')
  if (clip.locked || track.locked) throw new Error('Imported video is locked')
  if (clip.playbackMode?.mode === 'freeze') throw new Error('Freeze frame has no audio span')
  const audio: AudioClip = { id: ids.audioClipId, kind: 'audio', sourceId: clip.sourceId, timelineStartTicks: clip.timelineStartTicks, sourceInTicks: clip.sourceInTicks, sourceOutTicks: clip.sourceOutTicks, ...(clip.timelineDurationTicks !== undefined ? { timelineDurationTicks: clip.timelineDurationTicks } : {}), speed: clip.speed, ...(clip.speedRamp ? { speedRamp: clip.speedRamp } : {}), ...embeddedAudioMix(clip), reversed: clip.playbackMode?.mode === 'reverse', enabled: clip.enabled }
  let next = document
  if (!next.tracks.some((track) => track.id === ids.audioTrackId)) next = addTrack(next, { id: ids.audioTrackId, kind: 'audio', name: 'Audio', locked: false, muted: false, solo: false, clips: [] })
  next = addClip(next, ids.audioTrackId, audio)
  next = { ...next, tracks: next.tracks.map((track) => ({ ...track, clips: track.clips.map((item) => item.id === clipId ? { ...item, sourceAudioEnabled: false } : item) } as CompositionTrack)) }
  next = createCompositionLink(next, ids.linkId, [clipId, ids.audioClipId])
  assertValidComposition(next)
  assertCompositionLocksPreserved(document, next)
  return next
}
