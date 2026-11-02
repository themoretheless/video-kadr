import type { CompositionKeyframeTrack } from './types'
export interface KeyframeSampleClock { readonly rateNum: number; readonly rateDen: number; readonly offsetNum: number }
const limit = BigInt(Number.MAX_SAFE_INTEGER)
function gcd(a: bigint, b: bigint): bigint { while (b) { const r = a % b; a = b; b = r } return a }
export function canonicalSampleClock(value: unknown): KeyframeSampleClock {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(key => !['rateNum', 'rateDen', 'offsetNum'].includes(key))) throw new Error('Некорректные часы ключей')
  const c = value as KeyframeSampleClock
  if (![c.rateNum, c.rateDen, c.offsetNum].every(Number.isSafeInteger) || c.rateNum <= 0 || c.rateDen <= 0) throw new Error('Некорректные часы ключей')
  const absolute = BigInt(c.offsetNum) < 0n ? -BigInt(c.offsetNum) : BigInt(c.offsetNum)
  const common = gcd(gcd(BigInt(c.rateNum), BigInt(c.rateDen)), absolute)
  const n = BigInt(c.rateNum) / common, d = BigInt(c.rateDen) / common, o = BigInt(c.offsetNum) / common
  if (n > 86400000000n || d > 1000000n || o > limit || o < -limit) throw new Error('Часы ключей выходят за допустимый диапазон')
  return { rateNum: Number(n), rateDen: Number(d), offsetNum: Number(o) }
}
export function validSampleClock(value: unknown): boolean { try { canonicalSampleClock(value); return true } catch { return false } }
function rounded(n: bigint, d: bigint): number {
  let whole = n / d
  const remainder = n % d, abs = remainder < 0n ? -remainder : remainder
  if (abs * 2n >= d) whole += n < 0n ? -1n : 1n
  if (whole > limit || whole < -limit) throw new Error('Время ключей выходит за допустимый диапазон')
  return Number(whole)
}
/** Apply local-to-authored time before native key-grid rounding. */
export function sampleClockTick(track: CompositionKeyframeTrack, local: number, compositionBase: number): number {
  const clock = canonicalSampleClock(track.sampleClock)
  if (!Number.isSafeInteger(track.timeBase) || track.timeBase < 1 || track.timeBase > 100000000 || !Number.isSafeInteger(compositionBase) || compositionBase < 1 || !Number.isFinite(local)) throw new Error('Некорректное время ключей')
  if (Number.isSafeInteger(local)) {
    const n = (BigInt(local) * 1000000n * BigInt(clock.rateNum) + BigInt(clock.offsetNum) * BigInt(compositionBase)) * BigInt(track.timeBase)
    return rounded(n, BigInt(compositionBase) * 1000000n * BigInt(clock.rateDen))
  }
  const position = ((local / compositionBase * clock.rateNum + clock.offsetNum / 1000000) / clock.rateDen) * track.timeBase
  const result = Math.sign(position) * Math.floor(Math.abs(position) + 0.5)
  if (!Number.isSafeInteger(result)) throw new Error('Время ключей выходит за допустимый диапазон')
  return result
}

/** Move a curve in local time without quantizing its authored key grid. */
export function shiftSampleClock(value: KeyframeSampleClock, deltaTicks: number, compositionBase: number): KeyframeSampleClock {
  const clock = canonicalSampleClock(value)
  if (!Number.isSafeInteger(deltaTicks) || !Number.isSafeInteger(compositionBase) || compositionBase <= 0) throw new Error('Некорректный сдвиг ключей')
  const base = BigInt(compositionBase)
  const n = BigInt(clock.rateNum) * base
  const d = BigInt(clock.rateDen) * base
  const o = BigInt(clock.offsetNum) * base - BigInt(deltaTicks) * 1000000n * BigInt(clock.rateNum)
  const common = gcd(gcd(n, d), o < 0n ? -o : o)
  const reduced = [n / common, d / common, o / common]
  if (reduced.some(part => part > limit || part < -limit)) throw new Error('Сдвиг часов выходит за допустимый диапазон')
  return canonicalSampleClock({ rateNum: Number(reduced[0]), rateDen: Number(reduced[1]), offsetNum: Number(reduced[2]) })
}

/** Continuous local position of an authored key; do not round before screen projection. */
export function keyframeLocalTime(track: Pick<CompositionKeyframeTrack, 'timeBase' | 'sampleClock'>, tick: number, compositionBase = 1000000): number {
  if (!Number.isFinite(tick) || !Number.isSafeInteger(track.timeBase) || track.timeBase <= 0 || !Number.isSafeInteger(compositionBase) || compositionBase <= 0) throw new Error('Некорректное время ключей')
  if (!track.sampleClock) return tick * compositionBase / track.timeBase
  const c = canonicalSampleClock(track.sampleClock)
  if (Number.isSafeInteger(tick)) {
    const numerator = (BigInt(tick) * 1000000n * BigInt(c.rateDen) - BigInt(c.offsetNum) * BigInt(track.timeBase)) * BigInt(compositionBase)
    const denominator = BigInt(track.timeBase) * 1000000n * BigInt(c.rateNum)
    return Number(numerator) / Number(denominator)
  }
  return (tick / track.timeBase * c.rateDen - c.offsetNum / 1000000) * compositionBase / c.rateNum
}
/** Authored destination for local key editing; negative authored keys remain valid. */
export function keyframeAuthoredTick(track: CompositionKeyframeTrack, local: number, compositionBase = 1000000): number {
  if (track.sampleClock) return sampleClockTick(track, local, compositionBase)
  const tick = Math.round(local * track.timeBase / compositionBase)
  if (!Number.isSafeInteger(tick)) throw new Error('Некорректное время ключей')
  return tick
}

/** Continuous authored coordinate for graph pointers and range bounds. */
export function keyframeAuthoredTime(track: Pick<CompositionKeyframeTrack, 'timeBase' | 'sampleClock'>, local: number, compositionBase = 1000000): number {
  if (!track.sampleClock) return local * track.timeBase / compositionBase
  const c = canonicalSampleClock(track.sampleClock)
  return (local / compositionBase * c.rateNum + c.offsetNum / 1000000) * track.timeBase / c.rateDen
}
