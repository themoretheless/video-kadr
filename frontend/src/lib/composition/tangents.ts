import type { CompositionAnimatableValue, CompositionInterpolation, CompositionKeyframe, CompositionKeyframeTangents } from './types'

/** A user edit in normalized segment coordinates, independent of the owning editor. */
export type KeyframeTangentIntent =
  | { kind: 'mode'; mode: 'auto' | 'free' }
  | { kind: 'continuity'; continuity: 'smooth' | 'broken' }
  | { kind: 'point'; side: 'incoming' | 'outgoing'; x: number; y: number }

export function segmentInterpolation(left: CompositionKeyframe, right: CompositionKeyframe, fallback: CompositionInterpolation): CompositionInterpolation {
  const mode = left.interpolation ?? fallback
  if (!mode || typeof mode !== 'object' || mode.kind !== 'bezier') return mode
  return { ...mode,
    p1: left.tangents ? [left.tangents.outgoing.x, left.tangents.outgoing.y] : mode.p1,
    p2: right.tangents ? [right.tangents.incoming.x, right.tangents.incoming.y] : mode.p2,
  }
}
export function isKeyframeTangents(value: unknown): value is CompositionKeyframeTangents {
  if (!value || typeof value !== 'object') return false
  const record = value as Record<string, unknown>
  if (Object.keys(record).some(key => !['incoming','outgoing','continuity'].includes(key)) || !['smooth','broken'].includes(String(record.continuity))) return false
  return [record.incoming, record.outgoing].every(side => {
    if (!side || typeof side !== 'object') return false
    const t = side as Record<string, unknown>
    return Object.keys(t).every(key => ['x','y','mode'].includes(key)) &&
      typeof t.x === 'number' && Number.isFinite(t.x) && t.x >= 0 && t.x <= 1 &&
      typeof t.y === 'number' && Number.isFinite(t.y) && (t.mode === 'auto' || t.mode === 'free')
  })
}
export function sameKeyframeTangents(a: CompositionKeyframeTangents | undefined, b: CompositionKeyframeTangents | undefined): boolean {
  if (a === b) return true
  return !!a && !!b && a.continuity === b.continuity && (['incoming','outgoing'] as const).every(side =>
    a[side].x === b[side].x && a[side].y === b[side].y && a[side].mode === b[side].mode)
}
export function cloneInterpolation(value: CompositionInterpolation): CompositionInterpolation {
  if (typeof value !== 'object') return value
  return value.kind === 'bezier' ? { ...value,p1: [...value.p1],p2: [...value.p2] } : { ...value }
}
export function cloneKeyframe(key: CompositionKeyframe): CompositionKeyframe {
  return { ...key, ...(key.interpolation !== undefined ? { interpolation: cloneInterpolation(key.interpolation) } : {}), ...(key.tangents ? { tangents: { ...key.tangents, incoming: { ...key.tangents.incoming }, outgoing: { ...key.tangents.outgoing } } } : {}) }
}

/** Write-time twin of domain/tangents.rs. Never called by sampling or cuts. */
export function normalizeKeyframeTangents(keys: readonly CompositionKeyframe[], fallback: CompositionInterpolation): readonly CompositionKeyframe[] {
  const spline = (key: CompositionKeyframe) => { const mode = key.interpolation ?? fallback; return typeof mode === 'object' && mode.kind === 'bezier' }
  return keys.map((key, index) => {
    if (!key.tangents) return key
    if (!isKeyframeTangents(key.tangents)) throw new Error('Invalid keyframe tangents')
    let { incoming, outgoing } = key.tangents
    const previous = keys[index - 1]
    const next = keys[index + 1]
    const incomingSpline = !!previous && spline(previous)
    const outgoingSpline = !!next && spline(key)
    const dtPrevious = previous ? key.tick - previous.tick : 0
    const dtNext = next ? next.tick - key.tick : 0
    const dvPrevious = previous ? key.value - previous.value : 0
    const dvNext = next ? next.value - key.value : 0
    const slope = previous && next && dvPrevious !== 0 && dvNext !== 0 && Math.sign(dvPrevious) === Math.sign(dvNext) && next.tick > previous.tick
      ? (next.value - previous.value) / (next.tick - previous.tick) : 0
    const clamp = (v: number) => v <= 0 ? 0 : v >= 1 ? 1 : v
    if (outgoing.mode === 'auto' && outgoingSpline) outgoing = { x: 1/3, y: dvNext === 0 || dtNext <= 0 ? 1/3 : clamp(slope * dtNext / (3 * dvNext)), mode: 'auto' }
    if (incoming.mode === 'auto' && incomingSpline) incoming = { x: 2/3, y: dvPrevious === 0 || dtPrevious <= 0 ? 2/3 : clamp(1 - slope * dtPrevious / (3 * dvPrevious)), mode: 'auto' }
    if (key.tangents.continuity === 'smooth' && incoming.mode === 'free' && outgoing.mode === 'free' && incomingSpline && outgoingSpline && outgoing.x !== 0 && incoming.x !== 1 && dtNext > 0 && dtPrevious > 0 && dvPrevious !== 0) {
      const worldSlope = (outgoing.y / outgoing.x) * (dvNext / dtNext)
      incoming = { ...incoming, y: 1 - worldSlope * (1 - incoming.x) * dtPrevious / dvPrevious }
    }
    const tangents = { ...key.tangents, incoming, outgoing }
    if (!isKeyframeTangents(tangents)) throw new Error('Invalid resolved keyframe tangents')
    return sameKeyframeTangents(tangents, key.tangents) ? key : { ...key, tangents }
  })
}


/** Materialize both sides from their owning segments before changing metadata. */
export function resolvedKeyframeTangents(keys: readonly CompositionKeyframe[], index: number, fallback: CompositionInterpolation): CompositionKeyframeTangents {
  const key = keys[index]
  if (!key) throw new Error('Selected keyframe no longer exists')
  if (key.tangents) return { ...key.tangents, incoming: { ...key.tangents.incoming }, outgoing: { ...key.tangents.outgoing } }
  const previous = keys[index - 1]
  const before = previous ? previous.interpolation ?? fallback : fallback
  const after = key.interpolation ?? fallback
  const incoming = typeof before === 'object' && before.kind === 'bezier' ? before.p2 : [2/3,2/3]
  const outgoing = typeof after === 'object' && after.kind === 'bezier' ? after.p1 : [1/3,1/3]
  return { incoming: { x: incoming[0]!, y: incoming[1]!, mode: 'free' }, outgoing: { x: outgoing[0]!, y: outgoing[1]!, mode: 'free' }, continuity: 'broken' }
}

export function updateKeyframeTangents(value: CompositionAnimatableValue, ticks: readonly number[], patch: Partial<CompositionKeyframeTangents>): CompositionAnimatableValue {
  if (value.mode !== 'keyframes' || !ticks.length || ticks.some(tick => !value.track.keyframes.some(key => key.tick === tick))) throw new Error('Selected keyframe no longer exists')
  const keys = value.track.keyframes.map((key, index) => {
    if (!ticks.includes(key.tick)) return key
    const tangents = { ...resolvedKeyframeTangents(value.track.keyframes, index, value.track.interpolation), ...patch }
    if (!isKeyframeTangents(tangents)) throw new Error('Invalid keyframe tangents')
    return { ...key, tangents }
  })
  return { ...value, track: { ...value.track, keyframes: normalizeKeyframeTangents(keys, value.track.interpolation) } }
}

/** Dragging the incoming Free side must update outgoing before OUT WINS solve. */
export function editKeyframeTangent(value: CompositionAnimatableValue, tick: number, side: 'incoming' | 'outgoing', x: number, y: number): CompositionAnimatableValue {
  if (value.mode !== 'keyframes') throw new Error('Selected keyframe no longer exists')
  const keys = value.track.keyframes
  const index = keys.findIndex(key => key.tick === tick)
  const tangents = resolvedKeyframeTangents(keys, index, value.track.interpolation)
  const written = { x, y, mode: 'free' as const }
  let patch: Partial<CompositionKeyframeTangents> = { incoming: { ...tangents.incoming, mode: 'free' }, outgoing: { ...tangents.outgoing, mode: 'free' }, [side]: written }
  const previous = keys[index - 1]
  const next = keys[index + 1]
  const spline = (key: CompositionKeyframe) => { const mode = key.interpolation ?? value.track.interpolation; return typeof mode === 'object' && mode.kind === 'bezier' }
  if (side === 'incoming' && tangents.continuity === 'smooth' && previous && next && spline(keys[index]!) && x !== 1 && tangents.outgoing.x !== 0) {
    const dtPrevious = tick - previous.tick
    const dtNext = next.tick - tick
    const dvPrevious = keys[index]!.value - previous.value
    const dvNext = next.value - keys[index]!.value
    if (dtPrevious > 0 && dtNext > 0 && dvNext !== 0) {
      const slope = ((1-y)/(1-x))*(dvPrevious/dtPrevious)
      patch = { ...patch, outgoing: { ...tangents.outgoing, mode: 'free', y: slope * tangents.outgoing.x * dtNext / dvNext } }
    }
  }
  const segmentIndex = side === 'incoming' ? index-1 : index
  const converted = keys.map((key,i) => {
    if (segmentIndex < 0 || segmentIndex+1 >= keys.length) return key
    if (i !== segmentIndex && i !== segmentIndex+1) return key
    return { ...key, tangents: resolvedKeyframeTangents(keys,i,value.track.interpolation),
      ...(i === segmentIndex && !spline(key) ? { interpolation: { kind: 'bezier' as const, p1: [1/3,1/3] as const, p2: [2/3,2/3] as const } } : {}),
    }
  })
  return updateKeyframeTangents({ ...value,track: { ...value.track,keyframes: converted } }, [tick], patch)
}

export function setKeyframeTangentMode(value: CompositionAnimatableValue, ticks: readonly number[], mode: 'auto' | 'free'): CompositionAnimatableValue {
  if (value.mode !== 'keyframes' || !ticks.length || ticks.some(tick => !value.track.keyframes.some(key => key.tick === tick))) throw new Error('Selected keyframe no longer exists')
  const selected = new Set(ticks)
  const keys = value.track.keyframes.map((key,index) => {
    const owner = selected.has(key.tick)
    const adjacent = owner && index+1 < value.track.keyframes.length || !!value.track.keyframes[index+1] && selected.has(value.track.keyframes[index+1]!.tick)
    const touches = owner || mode === 'auto' && (adjacent || !!value.track.keyframes[index-1] && selected.has(value.track.keyframes[index-1]!.tick))
    const tangents = touches ? resolvedKeyframeTangents(value.track.keyframes,index,value.track.interpolation) : undefined
    return { ...key,
      ...(mode === 'auto' && adjacent ? { interpolation: { kind: 'bezier' as const, p1: [1/3,1/3] as const, p2: [2/3,2/3] as const } } : {}),
      ...(tangents ? { tangents: owner ? { ...tangents, incoming: { ...tangents.incoming, mode }, outgoing: { ...tangents.outgoing, mode }, ...(mode === 'auto' ? { continuity: 'smooth' as const } : {}) } : tangents } : {}),
    }
  })
  return { ...value, track: { ...value.track, keyframes: normalizeKeyframeTangents(keys,value.track.interpolation) } }
}

/** Called only after the authored animation envelope has passed schema admission. */
export function normalizeAuthoredTangentAnimation<T extends object>(animation: T): T {
  return Object.fromEntries(Object.entries(animation).map(([property, raw]) => {
    const value = raw as CompositionAnimatableValue
    return [property, value.mode === 'keyframes' ? { ...value,track: { ...value.track,keyframes: normalizeKeyframeTangents(value.track.keyframes,value.track.interpolation) } } : value]
  })) as T
}
