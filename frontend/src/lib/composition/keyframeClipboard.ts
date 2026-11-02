import { keyframeLocalTime, keyframeAuthoredTick } from './keyframeClock'
import { cloneKeyframe, normalizeKeyframeTangents } from './tangents'
import { COMPOSITION_TIME_BASE, type CompositionAnimatableValue, type CompositionKeyframe } from './types'

export interface KeyframeCopySubject {
  readonly parameter: string
  readonly value: CompositionAnimatableValue
  readonly selectedTicks: readonly number[]
  readonly timelineStartTicks: number
}
export interface KeyframeClipboardGroup {
  readonly parameter: string
  /** Composition clock, rebased against the earliest key in the whole copy. */
  readonly keys: readonly CompositionKeyframe[]
}

/** Independent authored snapshot; source clip identities are deliberately absent. */
export function snapshotSelectedKeyframes(subjects: readonly KeyframeCopySubject[]): KeyframeClipboardGroup[] {
  const entries: { parameter: string; key: CompositionKeyframe }[] = []
  for (const subject of subjects) {
    if (!Number.isSafeInteger(subject.timelineStartTicks) || subject.timelineStartTicks < 0) throw new Error('Invalid keyframe layer start')
    if (!subject.selectedTicks.length) continue
    if (subject.value.mode !== 'keyframes' || !Number.isSafeInteger(subject.value.track.timeBase) || subject.value.track.timeBase <= 0) throw new Error('Invalid copied keyframe track')
    for (const tick of subject.selectedTicks) {
      const key = subject.value.track.keyframes.find((candidate) => candidate.tick === tick)
      if (!key) throw new Error('Selected keyframe no longer exists')
      const compositionTick = Math.round(subject.timelineStartTicks + keyframeLocalTime(subject.value.track, key.tick))
      if (!Number.isSafeInteger(compositionTick)) throw new Error('Invalid copied keyframe time')
      entries.push({ parameter: subject.parameter, key: { ...cloneKeyframe(key), tick: compositionTick, interpolation: key.interpolation ?? subject.value.track.interpolation } })
    }
  }
  if (!entries.length) return []
  const first = Math.min(...entries.map((entry) => entry.key.tick))
  const groups = new Map<string, CompositionKeyframe[]>()
  for (const { parameter, key } of entries) {
    const keys = groups.get(parameter) ?? []
    keys.push({ ...key, tick: key.tick - first })
    groups.set(parameter, keys)
  }
  return [...groups].map(([parameter, keys]) => ({ parameter, keys: keys.sort((a, b) => a.tick - b.tick) }))
}

export interface KeyframePasteTarget {
  readonly id: string
  readonly parameter: string
  readonly value: CompositionAnimatableValue
  readonly timelineStartTicks: number
}
export interface KeyframePasteEntry {
  readonly id: string
  readonly parameter: string
  readonly value: CompositionAnimatableValue
  readonly pastedTicks: readonly number[]
}

/** Paste planning retains authored keys outside clip bounds; admission is separate. */
export function planKeyframePaste(groups: readonly KeyframeClipboardGroup[], targets: readonly KeyframePasteTarget[], atTicks: number): { entries: KeyframePasteEntry[]; skipped: string[] } {
  if (!Number.isSafeInteger(atTicks) || atTicks < 0) throw new Error('Invalid keyframe paste time')
  const addresses = targets.map((target) => JSON.stringify([target.id, target.parameter]))
  if (new Set(addresses).size !== addresses.length) throw new Error('Duplicate keyframe paste targets')
  const entries: KeyframePasteEntry[] = []
  const carried = new Set(targets.map((target) => target.parameter))
  const skipped = [...new Set(groups.filter((group) => !carried.has(group.parameter)).map((group) => group.parameter))]
  for (const target of targets) {
    const matching = groups.filter((group) => group.parameter === target.parameter)
    if (!matching.some((group) => group.keys.length)) continue
    if (!Number.isSafeInteger(target.timelineStartTicks) || target.timelineStartTicks < 0) throw new Error('Invalid keyframe target start')
    const keyed = target.value.mode === 'keyframes' ? target.value.track : null
    const timeBase = keyed?.timeBase ?? COMPOSITION_TIME_BASE
    if (!Number.isSafeInteger(timeBase) || timeBase <= 0) throw new Error('Invalid keyframe target time base')
    const landed = new Map<number, CompositionKeyframe>()
    for (const key of keyed?.keyframes ?? []) landed.set(key.tick, cloneKeyframe(key))
    const pastedTicks = new Set<number>()
    for (const group of matching) for (const key of group.keys) {
      if (!Number.isSafeInteger(key.tick) || key.tick < 0 || !Number.isFinite(key.value)) throw new Error('Invalid clipboard keyframe')
      const tick = keyed ? keyframeAuthoredTick(keyed, atTicks - target.timelineStartTicks + key.tick) : Math.round((atTicks - target.timelineStartTicks + key.tick) * timeBase / COMPOSITION_TIME_BASE)
      if (!Number.isSafeInteger(tick)) throw new Error('Invalid pasted keyframe time')
      landed.set(tick, { ...cloneKeyframe(key), tick })
      pastedTicks.add(tick)
    }
    entries.push({ id: target.id, parameter: target.parameter, pastedTicks: [...pastedTicks].sort((a, b) => a - b), value: { mode: 'keyframes', track: { ...keyed, timeBase, interpolation: keyed?.interpolation ?? 'linear', keyframes: normalizeKeyframeTangents([...landed.values()].sort((a, b) => a.tick - b.tick), keyed?.interpolation ?? 'linear') } } })
  }
  return { entries, skipped }
}

/** One editor slot; the most recent copy determines paste semantics. */
export type CompositionClipboardSlot = { readonly kind: 'color-keyframes'; readonly groups: readonly import('./colorClipboard').ColorClipboardGroup[] } | { readonly kind: 'keyframes'; readonly groups: readonly KeyframeClipboardGroup[] } | { readonly kind: 'layer'; readonly clipId: string; readonly insertSnapshot?: { readonly document: import('./types').Composition; readonly clipIds: readonly string[] } }
