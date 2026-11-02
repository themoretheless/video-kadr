import type { SourceTrimParts } from './concatSourceTrim'
import {windowNativeTreatment} from './nativeTreatmentWindow'
import {rewindowNativeClipSourceEffects} from './concatSourceWindow'
import { rebaseNativeClipAnimation } from './nativeClipAnimation'
import { CAPTION_SOURCE_TICKS_PER_MICROSECOND } from './captionTime'
import type { NativeNestedProject } from '$lib/api'
import { nativeClipEnd } from './nativeClipInsert'
import { expandNativeClipLinks } from './nativeClipLinks'
import { MAX_COMPOSITION_DURATION_TICKS, type WireCompositionTrack } from './types'
import { sliceCompositionSpeedRamp, speedRampSourceProgressAtTimelineTick, speedRampTimelineDurationTicks } from './speedRamp'
function clipStart(clip: WireCompositionTrack['clips'][number]): number {
  return 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
}
export function nativeClipEditBounds(clip: WireCompositionTrack['clips'][number]): readonly [number, number] {
  const clock = 'placement' in clip ? clip.placement.concatRetime?.documentClock : undefined
  return clock ? [clock.start, clock.start + clock.duration] : [clipStart(clip) / 1e6, nativeClipEnd(clip) / 1e6]
}
function intersects(a: WireCompositionTrack['clips'][number], b: WireCompositionTrack['clips'][number]): boolean {
  const [aStart, aEnd] = nativeClipEditBounds(a), [bStart, bEnd] = nativeClipEditBounds(b)
  return aStart < bEnd && aEnd > bStart
}
export function trimNativeClipEnds(project: NativeNestedProject, compositionId: string, ids: readonly string[], deltaUs: number, sourceParts?: SourceTrimParts): NativeNestedProject {
  const node = project.compositions[compositionId]
  if (!node || !Object.hasOwn(project.compositions, compositionId)) throw new Error('Таймлайн не найден')
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректный сдвиг границы')
  if (!ids.length || deltaUs === 0) return project
  if (new Set(ids).size !== ids.length) throw new Error('Клип выбран несколько раз')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  const tracks = node.content.tracks.map(track => {
    if (!track.clips.some(clip => selected.has(clip.id))) return track
    if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
    const clips = track.clips.map(clip => {
      if (!selected.has(clip.id)) return clip
      if ('placement' in clip && clip.placement.concatRetime) {
        const planned = sourceParts?.get(clip.id)
        if (!planned) throw new Error('Обрезка Concat требует нативного плана')
        return planned
      }
      const start = 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
      const end = nativeClipEnd(clip) + deltaUs, duration = end - start
      if (!Number.isSafeInteger(end) || duration <= 0 || end > MAX_COMPOSITION_DURATION_TICKS) throw new Error('Граница выходит за допустимый диапазон')
      clip=windowNativeTreatment(clip,0,duration) ?? rewindowNativeClipSourceEffects(clip,nativeClipEnd(clip)-start,0,duration)
      if (!('placement' in clip)) return 'durationTicks' in clip ? { ...clip, durationTicks: duration } : { ...clip, timelineEndTick: end }
      const p = clip.placement, source = node.content.sources[clip.sourceId]
      if (!source) throw new Error('Источник клипа не найден')
      if ('playbackMode' in clip && clip.playbackMode.mode === 'freeze') return { ...clip, placement: { ...p, timelineDurationTick: duration } }
      const reverse = 'playbackMode' in clip ? clip.playbackMode.mode === 'reverse' : 'reversed' in clip && clip.reversed
      const span = p.sourceOutTick - p.sourceInTick
      const phase = p.sourceTimelineOffsetTick ?? 0
      const derived = (p.speedRamp ? speedRampTimelineDurationTicks(span, p.speed, p.speedRamp) : Math.round(span / p.speed)) - phase
      // Preserve the full source window and ramp when cropping its presentation interval.
      // The native clock gates authored duration separately, so retained samples keep their phase.
      if (duration <= derived || nativeClipEnd(clip) - start > derived + 1) {
        const placement = { ...p, timelineDurationTick: duration }
        return { ...clip, placement }
      }
      let nextSpan = Math.max(1, Math.round((duration + phase) * p.speed)), speed = p.speed, speedRamp = p.speedRamp
      if (p.speedRamp) {
        nextSpan = Math.max(1, speedRampSourceProgressAtTimelineTick(span, p.speed, p.speedRamp, duration + phase))
        const sliced = sliceCompositionSpeedRamp(span, p.speed, p.speedRamp, 0, nextSpan)
        speed = sliced.speed; speedRamp = sliced.speedRamp
      }
      const sourceInTick = reverse ? p.sourceOutTick - nextSpan : p.sourceInTick
      const sourceOutTick = reverse ? p.sourceOutTick : p.sourceInTick + nextSpan
      if (sourceInTick < 0 || sourceOutTick > source.durationTicks || sourceOutTick <= sourceInTick) throw new Error('Не хватает исходного материала для удлинения')
      const placement = { ...p, sourceInTick, sourceOutTick, speed, ...(speedRamp ? { speedRamp } : {}), timelineDurationTick: duration }
      return { ...clip, placement }
    })
    // No ripple: extending into a neighbour refuses the complete linked transaction.
    for (let i = 0; i < clips.length; i++) {
      const a = clips[i]!
      if (!selected.has(a.id)) continue
      if (clips.some((b, j) => j !== i && intersects(a, b))) throw new Error('Новая граница пересекает соседний клип')
    }
    if (track.kind === 'video') {
      const transitions = track.transitions.map(transition => {
        if (transition.fromClipId && selected.has(transition.fromClipId)) {
          const shift = nativeClipEnd(clips.find(clip => clip.id === transition.fromClipId)!) - nativeClipEnd(track.clips.find(clip => clip.id === transition.fromClipId)!)
          if (!shift) return transition
          if (transition.toClipId) throw new Error('Связанный двухсторонний переход требует отдельного изменения границы')
          if (!transition.timeline) throw new Error('Переход не содержит независимого времени')
          return { ...transition, timeline: { ...transition.timeline, startTick: String(BigInt(transition.timeline.startTick) + BigInt(shift) * CAPTION_SOURCE_TICKS_PER_MICROSECOND) } }
        }
        if (transition.toClipId && selected.has(transition.toClipId) && transition.timeline) {
          const clip = clips.find(clip => clip.id === transition.toClipId)!
          if (BigInt(transition.timeline.startTick) + BigInt(transition.timeline.durationTick) > BigInt(nativeClipEnd(clip)) * CAPTION_SOURCE_TICKS_PER_MICROSECOND) throw new Error('Новый край пересекает входящий переход')
        }
        return transition
      })
      return { ...track, clips, transitions } as WireCompositionTrack
    }
    return { ...track, clips } as WireCompositionTrack
  })
  const changed = { ...node, content: { ...node.content, tracks } }
  if (node.durationUs !== undefined) changed.durationUs = Math.max(node.durationUs, ...tracks.flatMap(track => track.clips.map(nativeClipEnd)))
  return { ...project, compositions: { ...project.compositions, [compositionId]: changed } }
}

/** Move the presentation head inside the retained full source clock, keeping the end fixed. */
export function trimNativeClipStarts(project: NativeNestedProject, compositionId: string, ids: readonly string[], deltaUs: number, sourceParts?: SourceTrimParts): NativeNestedProject {
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректный сдвиг границы')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  if (!selected.size || deltaUs === 0) return project
  const node = project.compositions[compositionId]!
  const tracks = node.content.tracks.map(track => {
    const clips = track.clips.map(clip => {
      if (!selected.has(clip.id)) return clip
      if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
      if ('placement' in clip && clip.placement.concatRetime) {
        const planned = sourceParts?.get(clip.id)
        if (!planned) throw new Error('Обрезка Concat требует нативного плана')
        return planned
      }
      if ('effectTiming' in clip && clip.effectTiming) throw new Error('Исходные временные эффекты требуют отдельной обрезки')
      if ('graphic' in clip && (clip.responsiveTime || clip.roll)) throw new Error('Адаптивное время графики требует отдельной обрезки')
      const start = 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
      const end = nativeClipEnd(clip), nextStart = start+deltaUs, duration = end-nextStart
      if (!Number.isSafeInteger(nextStart) || nextStart < 0 || duration <= 0) throw new Error('Граница выходит за допустимый диапазон')
      const rebased = windowNativeTreatment(clip,deltaUs,end-start) ?? rebaseNativeClipAnimation(rewindowNativeClipSourceEffects(clip,end-start,deltaUs,end-start), -deltaUs)
      if ('placement' in clip && 'placement' in rebased) {
        const frozen = 'playbackMode' in clip && clip.playbackMode.mode === 'freeze'
        const phase = (clip.placement.sourceTimelineOffsetTick ?? 0)+deltaUs
        const headTime = Math.max(clip.placement.sourceHeadTimelineTick ?? 0, -phase, 0)
        if (!frozen && headTime > 0) {
          const source = node.content.sources[clip.sourceId]
          if (!source) throw new Error('Источник клипа не найден')
          const head = Math.ceil(headTime * clip.placement.speed)
          const reverse = 'playbackMode' in clip ? clip.playbackMode.mode === 'reverse' : 'reversed' in clip && clip.reversed
          if (!Number.isSafeInteger(headTime) || !Number.isSafeInteger(head) || (reverse ? clip.placement.sourceOutTick + head > source.durationTicks : head > clip.placement.sourceInTick)) throw new Error('Не хватает исходного материала для расширения')
        }
        const placement = { ...clip.placement, timelineStartTick: nextStart, timelineDurationTick: duration, ...(!frozen ? { sourceTimelineOffsetTick: phase, ...(headTime > 0 ? { sourceHeadTimelineTick: headTime } : {}) } : {}) }
        return { ...rebased, placement }
      }
      return 'durationTicks' in rebased ? { ...rebased, timelineStartTick: nextStart, durationTicks: duration } : { ...rebased, timelineStartTick: nextStart }
    })
    for (const clip of clips) if (selected.has(clip.id)) {
      if (clips.some(other => other.id !== clip.id && intersects(clip, other))) throw new Error('Новая граница пересекает соседний клип')
    }
    return { ...track, clips, ...(track.kind === 'video' ? { transitions: track.transitions.map(transition => {
      if (!transition.toClipId || !selected.has(transition.toClipId)) return transition
      const shift = clipStart(clips.find(clip => clip.id === transition.toClipId)!) - clipStart(track.clips.find(clip => clip.id === transition.toClipId)!)
      if (!shift) return transition
      if (transition.fromClipId) throw new Error('Двухсторонний переход требует отдельной обрезки')
      if (!transition.timeline) throw new Error('Переход не содержит независимого времени')
      return { ...transition, timeline: { ...transition.timeline, startTick: String(BigInt(transition.timeline.startTick)+BigInt(shift) * CAPTION_SOURCE_TICKS_PER_MICROSECOND) } }
    }) } : {}) } as WireCompositionTrack
  })
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, tracks } } } }
}
