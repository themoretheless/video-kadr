import {isConcatImageClip} from './concatImageSource'
import { planConcatSourceEdit, type ConcatSourceEditPlan, type ConcatSourceEditTracks, type NativeNestedProject } from '$lib/api'
import type { WireAudioClip, WireVideoClip, WireImageClip, WireConcatParamKey } from './types'
import { expandNativeClipLinks } from './nativeClipLinks'
import { splitNativeClips } from './nativeClipSplit'
import { rebaseNativeClipAnimation } from './nativeClipAnimation'

export type SourceClip = WireVideoClip | WireAudioClip
export type PlannedSourceClip = SourceClip | WireImageClip
type Runs = Record<string, readonly WireConcatParamKey[]>
export type SourceSplitParts = ReadonlyMap<string, readonly [PlannedSourceClip, PlannedSourceClip] | null>

// Paths identify the exact authored run, including nested bypass wrappers.
function effectRuns(value: unknown, path: string, visit: (path: string, run: readonly WireConcatParamKey[]) => readonly WireConcatParamKey[]): unknown {
  if (Array.isArray(value)) return value.map((item, index) => effectRuns(item, `${path}/${index}`, visit))
  if (!value || typeof value !== 'object') return value
  const object = value as Record<string, unknown>
  return Object.fromEntries(Object.entries(object).map(([key, entry]) => {
    const next = `${path}/${key}`
    if (key === 'concatKeys' || key === 'concatSourceKeys' || (key === 'keys' && path.endsWith('/link'))) {
      if (Array.isArray(entry)) return [key, visit(next, entry)]
      if (entry && typeof entry === 'object') return [key, Object.fromEntries(Object.entries(entry).map(([channel, run]) => [channel, Array.isArray(run) ? visit(`${next}/${channel}`, run) : run]))]
    }
    return [key, effectRuns(entry, next, visit)]
  }))
}
function tracksOf(clip: PlannedSourceClip): ConcatSourceEditTracks {
  const clipRuns: Runs = { ...('transform' in clip ? clip.transform.concatAnimation?.keys : {}) }
  if ('concatGainKeys' in clip && clip.concatGainKeys) clipRuns.volume = clip.concatGainKeys
  const audio: Runs = {}, video: Runs = {}
  effectRuns('dspEffects' in clip ? clip.dspEffects : undefined, 'audio', (path, run) => { audio[path] = run; return run })
  if ('effects' in clip) effectRuns(clip.effects, 'video', (path, run) => { video[path] = run; return run })
  return { clip: clipRuns, audio, video }
}
export function applyConcatSourcePart(clip: SourceClip, part: ConcatSourceEditPlan['parts'][number], deltaUs: number): SourceClip
export function applyConcatSourcePart(clip: WireImageClip, part: ConcatSourceEditPlan['parts'][number], deltaUs: number): WireImageClip
export function applyConcatSourcePart(clip: PlannedSourceClip, part: ConcatSourceEditPlan['parts'][number], deltaUs: number): PlannedSourceClip
export function applyConcatSourcePart(clip: PlannedSourceClip, part: ConcatSourceEditPlan['parts'][number], deltaUs: number): PlannedSourceClip {
  const next = rebaseNativeClipAnimation(clip, deltaUs)
  const required = (runs: ConcatSourceEditTracks['audio'], path: string) => {
    const run = runs?.[path]
    if (!run) throw new Error('План Concat потерял ключи эффекта')
    return run
  }
  if (!('placement' in next)) {
    const animation=next.transform.concatAnimation
    return {...next,timelineStartTick:part.placement.timelineStartTick,durationTicks:part.placement.timelineDurationTick!,concatSourceTiming:{sourceStart:part.clock.sourceStart,speed:part.clock.speed,...(part.clock.speedCurve?{speedCurve:part.clock.speedCurve}:{})},
      effects:effectRuns(next.effects,'video',path=>required(part.tracks.video,path)) as WireImageClip['effects'],
      transform:animation?{...next.transform,concatAnimation:{...animation,durationTicks:part.placement.timelineDurationTick!,keys:Object.fromEntries(Object.keys(animation.keys).map(name=>[name,required(part.tracks.clip,name)]))}}:next.transform}
  }
  const result = { ...next, placement: {...part.placement,...(next.placement.concatSourceEof?{concatSourceEof:true}:{})},
    dspEffects: effectRuns(next.dspEffects, 'audio', path => required(part.tracks.audio, path)) as SourceClip['dspEffects'],
    ...(next.concatGainKeys ? { concatGainKeys: required(part.tracks.clip, 'volume') } : {}) }
  if ('transform' in next) {
    const animation = next.transform.concatAnimation
    return { ...next, ...result,
      effects: effectRuns(next.effects, 'video', path => required(part.tracks.video, path)) as WireVideoClip['effects'],
      audioFadeInTicks: Math.round(part.fadeIn * 1e6), audioFadeOutTicks: Math.round(part.fadeOut * 1e6),
      transform: animation ? { ...next.transform, concatAnimation: { ...animation,
        durationTicks: part.placement.timelineDurationTick!, keys: Object.fromEntries(Object.keys(animation.keys).map(name => [name, required(part.tracks.clip, name)])) } } : next.transform }
  }
  return { ...result, fadeInTicks: Math.round(part.fadeIn * 1e6), fadeOutTicks: Math.round(part.fadeOut * 1e6), crossfadeInTicks: part.role === 'tail' ? 0 : next.crossfadeInTicks } as WireAudioClip
}

/** Plan all linked source participants before changing the immutable project. */
export async function splitNativeClipsWithSourcePlans(project: NativeNestedProject, compositionId: string, ids: readonly string[], timeUs: number, mint: () => string): Promise<NativeNestedProject> {
  if (!Number.isSafeInteger(timeUs) || timeUs < 0) throw new Error('Некорректное время разреза')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  const node = project.compositions[compositionId]!, parts = new Map<string, readonly [PlannedSourceClip, PlannedSourceClip] | null>()
  for (const track of node.content.tracks) for (const clip of track.clips) {
    if (!selected.has(clip.id) || !(isConcatImageClip(clip)||('placement' in clip&&clip.placement.concatRetime))) continue
    if (!('sourceId' in clip) || !('dspEffects' in clip || 'gain' in clip || 'transform' in clip)) throw new Error('Неподдерживаемый исходный клип Concat')
    const source = clip as PlannedSourceClip, input = concatSourceEditInput(source, node.content.canvas), clock = input.clock
    if (timeUs / 1e6 <= clock.start || timeUs / 1e6 >= clock.start + clock.duration) { parts.set(clip.id, null); continue }
    if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
    const plan = await planConcatSourceEdit({ ...input,
      command: { kind: 'split', time: timeUs / 1e6 } })
    if (!plan.changed) { parts.set(clip.id, null); continue }
    const [head, tail] = plan.parts
    if (plan.parts.length !== 2 || head?.role !== 'head' || tail?.role !== 'tail' || plan.rippleBy !== 0 || !head.placement.concatRetime?.documentClock || !tail.placement.concatRetime?.documentClock) throw new Error('Некорректный план разреза Concat')
    parts.set(clip.id, [applyConcatSourcePart(source, head, 0), applyConcatSourcePart(source, tail, ('placement' in source?source.placement.timelineStartTick:source.timelineStartTick) - tail.placement.timelineStartTick)])
  }
  return splitNativeClips(project, compositionId, ids, timeUs, mint, parts)
}

export function concatSourceEditInput(clip: PlannedSourceClip, canvas: import('./types').WireCanvas) {
  if (!('placement' in clip)) {
    const timing=clip.concatSourceTiming??{sourceStart:0,speed:1}
    const rate=canvas.fpsRate??{numerator:canvas.fpsMilli,denominator:1000}
    return {clock:{start:clip.timelineStartTick/1e6,duration:clip.durationTicks/1e6,sourceStart:timing.sourceStart,speed:timing.speed,...(timing.speedCurve?{speedCurve:timing.speedCurve}:{})},rateNum:rate.numerator,rateDen:rate.denominator,tracks:tracksOf(clip),fadeIn:0,fadeOut:0}
  }
  const source = clip as SourceClip, retained = source.placement.concatRetime!, clock = retained.documentClock
  if (!clock) throw new Error('Нужны исходные часы документа Concat')
  const p = source.placement, points = clock.speedCurve ?? []
  if (p.timelineStartTick !== Math.round(clock.start * 1e6) || p.timelineDurationTick !== Math.round(clock.duration * 1e6) || retained.durationTicks !== p.timelineDurationTick || p.sourceInTick !== Math.round(clock.sourceStart * 1e6) || p.speed !== clock.speed || p.speedRamp || points.length !== retained.points.length || points.some((point, i) => point.at !== retained.points[i]![0] || point.speed !== retained.points[i]![1])) throw new Error('Исходные часы Concat не соответствуют клипу')
  if ('effectTiming' in source && source.effectTiming) throw new Error('Исходные временные эффекты требуют отдельного редактирования')
  const rate = canvas.fpsRate ?? { numerator: canvas.fpsMilli, denominator: 1000 }
  if (retained.rateNum * rate.denominator !== rate.numerator * retained.rateDen) throw new Error('Частота кадров Concat изменилась')
  if (source.placement.sourceTimelineOffsetTick || source.placement.sourceHeadTimelineTick) throw new Error('Требуется полное исходное окно Concat')
  return { clock, rateNum: retained.rateNum, rateDen: retained.rateDen, tracks: tracksOf(source),
    fadeIn: ('audioFadeInTicks' in source ? source.audioFadeInTicks ?? 0 : 'fadeInTicks' in source ? source.fadeInTicks : 0) / 1e6,
    fadeOut: ('audioFadeOutTicks' in source ? source.audioFadeOutTicks ?? 0 : 'fadeOutTicks' in source ? source.fadeOutTicks : 0) / 1e6 }
}
