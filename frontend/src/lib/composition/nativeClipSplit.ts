import {isConcatImageClip} from './concatImageSource'
import type { SourceSplitParts } from './concatSourceSplit'
import {windowNativeTreatment} from './nativeTreatmentWindow'
import {rewindowNativeClipSourceEffects} from './concatSourceWindow'
import type { NativeNestedProject } from '$lib/api'
import { expandNativeClipLinks } from './nativeClipLinks'
import { nativeClipEnd } from './nativeClipInsert'
import { rebaseNativeClipAnimation } from './nativeClipAnimation'
import { MAX_COMPOSITION_CLIPS, type WireCompositionTrack } from './types'
export function splitNativeClips(project: NativeNestedProject, compositionId: string, ids: readonly string[], timeUs: number, mint: () => string, sourceParts?: SourceSplitParts): NativeNestedProject {
  if (!Number.isSafeInteger(timeUs) || timeUs < 0) throw new Error('Некорректное время разреза')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  const node = project.compositions[compositionId]!
  const reserved = new Set(node.content.tracks.flatMap(track => track.clips.map(clip => clip.id)))
  const tails = new Map<string, string>()
  const tracks = node.content.tracks.map(track => {
    const clips = track.clips.flatMap(clip => {
      const start = 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
      const end = nativeClipEnd(clip)
      if (!selected.has(clip.id)) return [clip]
      const source = isConcatImageClip(clip)||('placement' in clip && clip.placement.concatRetime)
      if (source) {
        if (!sourceParts?.has(clip.id)) throw new Error('Разрез Concat требует нативного плана')
        const parts = sourceParts.get(clip.id)
        if (!parts) return [clip]
        if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
        const id = mint()
        if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(id) || reserved.has(id)) throw new Error('Повторный или некорректный идентификатор клипа')
        reserved.add(id); tails.set(clip.id, id)
        return [parts[0], { ...parts[1], id }]
      }
      if (timeUs <= start || timeUs >= end) return [clip]
      if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
      if ('effectTiming' in clip && clip.effectTiming) throw new Error('Разрез исходных временных эффектов ещё не поддержан')
      if ('graphic' in clip && (clip.responsiveTime || clip.roll)) throw new Error('Разрез адаптивного времени графики ещё не поддержан')
      const id = mint()
      if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(id) || reserved.has(id)) throw new Error('Повторный или некорректный идентификатор клипа')
      reserved.add(id); tails.set(clip.id, id)
      const cut = timeUs - start, duration = end - start
      const leftSource=windowNativeTreatment(clip,0,cut,'head') ?? rewindowNativeClipSourceEffects(clip,duration,0,cut)
      const rebased = windowNativeTreatment(clip,cut,duration,'tail') ?? rebaseNativeClipAnimation(rewindowNativeClipSourceEffects(clip,duration,cut,duration), -cut)
      if ('placement' in clip && 'placement' in rebased) {
        const left = { ...leftSource, placement: { ...clip.placement, timelineDurationTick: cut } }
        const right = { ...rebased, id, placement: { ...clip.placement, timelineStartTick: timeUs, timelineDurationTick: duration-cut, sourceTimelineOffsetTick: 'playbackMode' in clip && clip.playbackMode.mode === 'freeze' ? clip.placement.sourceTimelineOffsetTick : (clip.placement.sourceTimelineOffsetTick ?? 0)+cut } }
        if ('playbackMode' in left && 'playbackMode' in right) {
          left.audioFadeInTicks=left.audioFadeInTicks??0;left.audioFadeOutTicks=0
          right.audioFadeInTicks=0;right.audioFadeOutTicks=right.audioFadeOutTicks??0
        }
        if ('fadeInTicks' in left && 'fadeInTicks' in right) {
          left.fadeOutTicks = 0
          right.fadeInTicks = 0; right.crossfadeInTicks = 0
        }
        return [left, right]
      }
      if ('timelineStartTick' in clip && 'timelineStartTick' in rebased) {
        return 'durationTicks' in clip ? [{ ...leftSource, durationTicks: cut }, { ...rebased, id, timelineStartTick: timeUs, durationTicks: duration-cut }] : [{ ...leftSource, timelineEndTick: timeUs }, { ...rebased, id, timelineStartTick: timeUs, timelineEndTick: end }]
      }
      throw new Error('Неподдерживаемый клип')
    })
    return { ...track, clips, ...(track.kind === 'video' ? { transitions: track.transitions.map(transition => transition.fromClipId && tails.has(transition.fromClipId) ? { ...transition, fromClipId: tails.get(transition.fromClipId)! } : transition) } : {}) } as WireCompositionTrack
  })
  if (!tails.size) return project
  if (tracks.reduce((count, track) => count+track.clips.length, 0)+(node.content.captionTracks ?? []).reduce((count, track) => count+track.clips.length, 0) > MAX_COMPOSITION_CLIPS) throw new Error('Слишком много клипов')
  const metadata = { ...node.content.metadata }
  for (const field of ['clipNames', 'mediaVersions'] as const) {
    const values = metadata[field]
    if (values) Object.assign(metadata, { [field]: Object.fromEntries([...Object.entries(values), ...[...tails].filter(([id]) => Object.hasOwn(values, id)).map(([id, tail]) => [tail, values[id]])]) })
  }
  if (metadata.clipLinks) metadata.clipLinks = Object.fromEntries(Object.entries(metadata.clipLinks).map(([id, members]) => [id, members.flatMap(member => tails.has(member) ? [member, tails.get(member)!] : [member])]))
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, tracks, metadata } } } }
}
