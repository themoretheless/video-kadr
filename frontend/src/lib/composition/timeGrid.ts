import type { CompositionCanvas } from './types'
/** Canonical boundaries use integer rational arithmetic, including fractional video rates. */
export interface TimeGridRate { numerator: number; denominator: number }
export const AUDIO_TIME_GRID: TimeGridRate = { numerator: 48_000, denominator: 1 }
const TICKS_PER_SECOND = 1_000_000n

function rateParts(rate: TimeGridRate): [bigint, bigint] {
  if (!Number.isSafeInteger(rate.numerator) || rate.numerator <= 0 || !Number.isSafeInteger(rate.denominator) || rate.denominator <= 0) throw new Error('Invalid time grid rate')
  return [BigInt(rate.numerator), BigInt(rate.denominator)]
}
function safeNumber(value: bigint): number {
  const result = Number(value)
  if (!Number.isSafeInteger(result)) throw new Error('Time grid boundary exceeds safe ticks')
  return result
}
function roundRatio(value: bigint, divisor: bigint): bigint { return (value * 2n + divisor) / (divisor * 2n) }

export function gridBoundaryTicks(index: number, rate: TimeGridRate): number {
  if (!Number.isSafeInteger(index) || index < 0) throw new Error('Invalid time grid index')
  const [numerator, denominator] = rateParts(rate)
  return safeNumber(roundRatio(BigInt(index) * TICKS_PER_SECOND * denominator, numerator))
}

export function snapTimeGridTicks(tick: number, rate: TimeGridRate): number {
  if (!Number.isSafeInteger(tick) || tick < 0) throw new Error('Invalid time grid tick')
  const [numerator, denominator] = rateParts(rate)
  const index = safeNumber(roundRatio(BigInt(tick) * numerator, TICKS_PER_SECOND * denominator))
  return gridBoundaryTicks(index, rate)
}

/** Shift both endpoints independently: a duration is not itself a grid boundary. */
export function shiftTimeGridSpan(start: number, end: number, delta: number, rate: TimeGridRate): { start: number; end: number } {
  if (!Number.isSafeInteger(delta) || end <= start) throw new Error('Invalid time grid span')
  const landed = { start: snapTimeGridTicks(start + delta, rate), end: snapTimeGridTicks(end + delta, rate) }
  if (landed.end <= landed.start) throw new Error('Time grid landing collapses the clip span')
  return landed
}

/** Legacy numeric rates remain their authored decimal rate, with no implicit NTSC substitution. */
export function canvasTimeGrid(canvas: Pick<CompositionCanvas, 'fps' | 'fpsRate'>): TimeGridRate {
  if (canvas.fpsRate) { rateParts(canvas.fpsRate); return canvas.fpsRate }
  const numerator = Math.round(canvas.fps * 1_000_000)
  const denominator = 1_000_000
  rateParts({ numerator, denominator })
  return { numerator, denominator }
}

export function stepTimeGridTicks(tick: number, direction: -1 | 1, rate: TimeGridRate): number {
  if (!Number.isSafeInteger(tick) || tick < 0) throw new Error('Invalid time grid tick')
  const [numerator, denominator] = rateParts(rate)
  const index = safeNumber(roundRatio(BigInt(tick) * numerator, TICKS_PER_SECOND * denominator))
  return gridBoundaryTicks(Math.max(0, index + direction), rate)
}

export interface TimeGridMember { id: string; kind: 'video' | 'audio' | 'image' | 'text' | 'color' | 'graphic'; start: number; end: number }
/** One total shift, one rounding per endpoint, and a common origin floor for the entire set. */
export function shiftTimeGridMembers(members: readonly TimeGridMember[], delta: number, visualRate: TimeGridRate): Map<string, { start: number; end: number }> {
  if (!Number.isSafeInteger(delta) || members.length > 512 || new Set(members.map((member) => member.id)).size !== members.length) throw new Error('Invalid time grid subjects')
  rateParts(visualRate)
  if (!members.length) return new Map()
  for (const member of members) {
    if (!member.id || !['video', 'audio', 'image', 'text', 'color', 'graphic'].includes(member.kind) || !Number.isSafeInteger(member.start) || member.start < 0 || !Number.isSafeInteger(member.end) || member.end <= member.start) throw new Error('Invalid time grid member')
  }
  const flooredDelta = Math.max(delta, -Math.min(...members.map((member) => member.start)))
  return new Map(members.map((member) => [member.id, shiftTimeGridSpan(member.start, member.end, flooredDelta, member.kind === 'audio' ? AUDIO_TIME_GRID : visualRate)]))
}
