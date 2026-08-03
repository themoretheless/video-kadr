import { describe, expect, it } from 'vitest'
import { defaultTimecodeRate, parseSmpteTimecode, parseSyncClock } from './multicam-sync-input'

describe('multicam sync input', () => {
  it('parses bounded marker clocks to integer project ticks', () => {
    expect(parseSyncClock('01:02.250', 1_000, 70_000)).toBe(62_250)
    expect(parseSyncClock('00:01:02.250', 1_000, 70_000)).toBe(62_250)
    expect(() => parseSyncClock('00:70.000', 1_000, 100_000)).toThrow(/формат/)
    expect(() => parseSyncClock('00:10.000', 1_000, 10_000)).toThrow(/вне источника/)
  })

  it('parses non-drop and legal drop-frame SMPTE values', () => {
    expect(parseSmpteTimecode('00:00:10:12', '25', false)).toMatchObject({ startFrame: 262, rate: { numerator: 25, denominator: 1 } })
    expect(parseSmpteTimecode('00:01:00;02', '29.97', true).startFrame).toBe(1_800)
    expect(parseSmpteTimecode('00:10:00;00', '29.97', true).startFrame).toBe(17_982)
    expect(() => parseSmpteTimecode('00:01:00;00', '29.97', true)).toThrow(/пропускается/)
    expect(() => parseSmpteTimecode('00:00:00;00', '25', true)).toThrow(/Drop-frame/)
  })

  it('chooses a deterministic supported rate from probed fps', () => {
    expect(defaultTimecodeRate(29.9701)).toBe('29.97')
    expect(defaultTimecodeRate(undefined)).toBe('25')
  })
})
