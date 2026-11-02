import { keyframeLocalTime, keyframeAuthoredTick } from './keyframeClock'
import { normalizeKeyframeTangents } from './tangents'
import { COMPOSITION_TIME_BASE, type CompositionAnimatableValue } from './types'
import { gridBoundaryTicks, snapTimeGridTicks, type TimeGridRate } from './timeGrid'

export interface SelectedKeyframeTrack {
  readonly id: string
  readonly value: CompositionAnimatableValue
  readonly selectedTicks: readonly number[]
  readonly durationTicks: number
}

/** One frame-snapped translation, constrained by the tightest selected layer wall. */
export function moveSelectedKeyframeTracks(groups: readonly SelectedKeyframeTrack[], requestedDelta: number, grid: TimeGridRate): { delta: number; groups: SelectedKeyframeTrack[] } {
  if (!Number.isSafeInteger(requestedDelta) || new Set(groups.map((group) => group.id)).size !== groups.length) throw new Error('Invalid keyframe move subjects')
  let minimum = -Infinity
  let maximum = Infinity
  let count = 0
  for (const group of groups) {
    if (group.value.mode !== 'keyframes' || !Number.isSafeInteger(group.value.track.timeBase) || group.value.track.timeBase <= 0 || group.selectedTicks.some((tick) => !Number.isSafeInteger(tick)) || !Number.isSafeInteger(group.durationTicks) || group.durationTicks < 0 || new Set(group.selectedTicks).size !== group.selectedTicks.length) throw new Error('Invalid selected keyframe track')
    for (const tick of group.selectedTicks) {
      if (!group.value.track.keyframes.some((key) => key.tick === tick)) throw new Error('Selected keyframe no longer exists')
      const local = keyframeLocalTime(group.value.track, tick)
      minimum = Math.max(minimum, -local)
      maximum = Math.min(maximum, group.durationTicks - local)
      count++
    }
  }
  if (!count) throw new Error('Select keyframes before moving')
  if (minimum > maximum) return { delta: 0, groups: [...groups] }
  const snapped = Math.sign(requestedDelta) * snapTimeGridTicks(Math.abs(requestedDelta), grid)
  const delta = Math.max(Math.ceil(minimum), Math.min(Math.floor(maximum), snapped)) || 0
  return { delta, groups: groups.map((group) => {
    if (group.value.mode !== 'keyframes' || !group.selectedTicks.length) return group
    const track = group.value.track
    const selected = new Set(group.selectedTicks)
    const moved = track.keyframes.filter((key) => selected.has(key.tick)).map((key) => ({ ...key, tick: keyframeAuthoredTick(track, keyframeLocalTime(track, key.tick) + delta) }))
    if (new Set(moved.map((key) => key.tick)).size !== moved.length) throw new Error('Keyframe move collapses selected keys')
    const destinations = new Set(moved.map((key) => key.tick))
    const keyframes = [...track.keyframes.filter((key) => !selected.has(key.tick) && !destinations.has(key.tick)), ...moved].sort((a, b) => a.tick - b.tick)
    return { ...group, selectedTicks: moved.map((key) => key.tick), value: { mode: 'keyframes', track: { ...track, keyframes: normalizeKeyframeTangents(keyframes, track.interpolation) } } }
  }) }
}

/** Scale composition-time keys about one anchor; round each result only after scaling. */
export function scaleSelectedKeyframeTracks(groups: readonly (SelectedKeyframeTrack & { timelineStartTicks?: number })[], anchorTicks: number, factor: number, grid: TimeGridRate, options: { constrainToClips?: boolean } = {}): { factor: number; groups: SelectedKeyframeTrack[] } {
  if (!Number.isSafeInteger(anchorTicks) || !Number.isFinite(factor)) throw new Error('Invalid keyframe scale')
  // Reuse translation's complete subject/time validation without changing its result.
  moveSelectedKeyframeTracks(groups, 0, grid)
  const times = groups.flatMap((group) => group.selectedTicks.map((tick) => (group.timelineStartTicks ?? 0) + (group.value.mode === 'keyframes' ? keyframeLocalTime(group.value.track, tick) : tick * COMPOSITION_TIME_BASE))).sort((a, b) => a - b)
  const distinct = [...new Set(times)]
  if (distinct.length < 2) return { factor: 1, groups: [...groups] }
  let gap = Infinity
  for (let index = 1; index < distinct.length; index++) gap = Math.min(gap, distinct[index]! - distinct[index - 1]!)
  let minimum = gridBoundaryTicks(1, grid) / gap
  let maximum = Infinity
  for (const group of groups) {
    if (group.value.mode !== 'keyframes') continue
    const start = group.timelineStartTicks ?? 0
    if (!Number.isSafeInteger(start) || start < 0) throw new Error('Invalid keyframe layer start')
    const anchor = anchorTicks - start
    for (const tick of group.selectedTicks) {
      const delta = keyframeLocalTime(group.value.track, tick) - anchor
      if (delta > 0) {
        if(options.constrainToClips)minimum = Math.max(minimum, -anchor / delta)
        maximum = Math.min(maximum, (group.durationTicks - anchor) / delta)
      } else if (delta < 0) {
        if(options.constrainToClips)minimum = Math.max(minimum, (group.durationTicks - anchor) / delta)
        maximum = Math.min(maximum, -anchor / delta)
      } else if (options.constrainToClips && (anchor < 0 || anchor > group.durationTicks)) throw new Error('Keyframe scale anchor is outside the clip')
    }
  }
  if (options.constrainToClips && maximum < minimum) throw new Error('Keyframe scale cannot fit inside selected clips')
  const applied = Math.min(Math.max(factor, minimum), Math.max(minimum, maximum))
  if (applied === 1) return { factor: 1, groups: [...groups] }
  return { factor: applied, groups: groups.map((group) => {
    if (group.value.mode !== 'keyframes' || !group.selectedTicks.length) return group
    const track = group.value.track
    const selected = new Set(group.selectedTicks)
    const anchor = anchorTicks - (group.timelineStartTicks ?? 0)
    const moved = track.keyframes.filter((key) => selected.has(key.tick)).map((key) => {
      const original = keyframeLocalTime(track, key.tick)
      // A native anchor is canonical microseconds; retain the authored phase of
      // keys at that anchor rather than snapping the fixed point onto a frame.
      if (options.constrainToClips && Math.abs(original - anchor) <= 0.5) return key
      const local = anchor + (original - anchor) * applied
      const snapped = Math.sign(local) * snapTimeGridTicks(Math.abs(Math.round(local)), grid)
      return { ...key, tick: keyframeAuthoredTick(track, options.constrainToClips ? Math.max(0, Math.min(group.durationTicks, snapped)) : snapped) }
    })
    if (new Set(moved.map((key) => key.tick)).size !== moved.length) throw new Error('Keyframe scale collapses selected keys')
    const destinations = new Set(moved.map((key) => key.tick))
    const keyframes = [...track.keyframes.filter((key) => !selected.has(key.tick) && !destinations.has(key.tick)), ...moved].sort((a, b) => a.tick - b.tick)
    return { ...group, selectedTicks: moved.map((key) => key.tick), value: { mode: 'keyframes', track: { ...track, keyframes: normalizeKeyframeTangents(keyframes, track.interpolation) } } }
  }) }
}
