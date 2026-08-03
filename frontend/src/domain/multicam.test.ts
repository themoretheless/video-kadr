import { describe, expect, it } from 'vitest'

import type { ProjectMedia } from '../project-schema'
import {
  activeMulticamAngle,
  compileFlattenedMulticamIntervals,
  correlateMulticamAudio,
  createMulticamGroup,
  deleteMulticamDecision,
  insertOrReplaceMulticamDecision,
  multicamFingerprintInput,
  MulticamError,
  syncMulticamByMarkers,
  syncMulticamByTimecode,
  validateMulticamDecisions,
  validateMulticamGroup,
  type MulticamDecision,
} from './multicam'

const fingerprint = (digit: string) => digit.repeat(64)
function media(id: string, duration = 10, hash = 'a'): ProjectMedia {
  return { id, kind: 'video', assetRef: id, contentFingerprint: fingerprint(hash), metadata: { filename: `${id}.mp4`, duration } }
}
const sources = [media('camera-a', 12, 'a'), media('camera-b', 10, 'b'), media('camera-c', 11, 'c')]
const group = () => createMulticamGroup({ id: 'group-1', name: 'Interview', timeBase: 1_000, media: sources })
const decisions = (): MulticamDecision[] => [
  { id: 'cut-0', offsetTick: 0, angleId: 'angle-1' },
  { id: 'cut-1', offsetTick: 2_000, angleId: 'angle-2' },
  { id: 'cut-2', offsetTick: 5_000, angleId: 'angle-3' },
]

describe('multicam domain', () => {
  it('creates a bounded deterministic group from distinct video media', () => {
    const value = group()
    expect(value).toMatchObject({ contract: 'multicam-v1', durationTicks: 10_000, referenceAngleId: 'angle-1' })
    expect(value.angles.map(angle => [angle.id, angle.mediaId])).toEqual([
      ['angle-1', 'camera-a'], ['angle-2', 'camera-b'], ['angle-3', 'camera-c'],
    ])
    expect(() => createMulticamGroup({ id: 'x', name: 'x', timeBase: 1_000, media: [sources[0]!] }))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'invalid_group' }))
    expect(() => createMulticamGroup({ id: 'x', name: 'x', timeBase: 1_000, media: [sources[0]!, sources[0]!] }))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'invalid_media' }))
  })

  it('validates references, reduced rates and source bounds fail closed', () => {
    const value = group()
    validateMulticamGroup(value, sources)
    expect(() => validateMulticamGroup({ ...value, referenceAngleId: 'gone' }, sources)).toThrow()
    expect(() => validateMulticamGroup({ ...value, angles: value.angles.map((angle, index) => index ? angle : { ...angle, rate: { numerator: 2, denominator: 2 } }) }, sources)).toThrow()
    expect(() => validateMulticamGroup({ ...value, durationTicks: 20_000 }, sources))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'out_of_bounds' }))
  })

  it('syncs every angle from markers and shortens to the common source window', () => {
    const synced = syncMulticamByMarkers(group(), [
      { angleId: 'angle-1', sourceTick: 1_000 },
      { angleId: 'angle-2', sourceTick: 2_000 },
      { angleId: 'angle-3', sourceTick: 500 },
    ], sources)
    expect(synced.sync).toMatchObject({ method: 'marker', algorithmVersion: 'marker-anchor-v1' })
    expect(synced.durationTicks).toBe(8_000)
    expect(synced.angles.map(angle => angle.sourceOriginTick)).toEqual([1_000, 2_000, 500])
    expect(() => syncMulticamByMarkers(group(), [{ angleId: 'angle-1', sourceTick: 0 }], sources)).toThrow()
  })

  it('syncs rational timecodes without using wall-clock ordering', () => {
    const synced = syncMulticamByTimecode(group(), [
      { angleId: 'angle-1', startFrame: 300, rate: { numerator: 30, denominator: 1 }, dropFrame: false },
      { angleId: 'angle-2', startFrame: 240, rate: { numerator: 30, denominator: 1 }, dropFrame: false },
      { angleId: 'angle-3', startFrame: 600, rate: { numerator: 60, denominator: 1 }, dropFrame: false },
    ], sources)
    expect(synced.angles.map(angle => angle.sourceOriginTick)).toEqual([0, 2_000, 0])
    expect(synced.durationTicks).toBe(8_000)
    expect(() => syncMulticamByTimecode(group(), [
      { angleId: 'angle-1', startFrame: 0, rate: { numerator: 0, denominator: 1 }, dropFrame: false },
    ], sources)).toThrow()
  })

  it('finds a unique bounded audio lag and rejects silence, ambiguity and excess work', () => {
    const reference = Float32Array.from({ length: 256 }, (_, index) => Math.sin(index * 0.31) + (index === 90 ? 3 : 0))
    const candidate = new Float32Array(256)
    candidate.set(reference.slice(0, 249), 7)
    const result = correlateMulticamAudio(reference, candidate, 16)
    expect(result.offsetSamples).toBe(7)
    expect(result.peak).toBeGreaterThan(0.9)
    expect(result.confidence).toBeGreaterThan(0.05)
    expect(() => correlateMulticamAudio(new Float32Array(256), new Float32Array(256), 16))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'ambiguous_audio' }))
    expect(() => correlateMulticamAudio(new Float32Array(100_000), new Float32Array(100_000), 100))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'audio_budget' }))
  })

  it('inserts, replaces, coalesces and deletes frame-aligned decisions', () => {
    const value = group(); const initial = decisions()
    validateMulticamDecisions(value, initial, 100)
    const inserted = insertOrReplaceMulticamDecision(value, initial, { id: 'cut-new', offsetTick: 3_000, angleId: 'angle-3' }, 100)
    expect(inserted.map(item => [item.offsetTick, item.angleId])).toEqual([
      [0, 'angle-1'], [2_000, 'angle-2'], [3_000, 'angle-3'],
    ])
    const replaced = insertOrReplaceMulticamDecision(value, initial, { id: 'replacement', offsetTick: 2_000, angleId: 'angle-1' }, 100)
    expect(replaced.map(item => item.offsetTick)).toEqual([0, 5_000])
    expect(deleteMulticamDecision(value, initial, 'cut-1', 100).map(item => item.angleId)).toEqual(['angle-1', 'angle-3'])
    expect(() => deleteMulticamDecision(value, initial, 'cut-0', 100)).toThrow()
    expect(() => insertOrReplaceMulticamDecision(value, initial, { id: 'bad', offsetTick: 2_050, angleId: 'angle-2' }, 100)).toThrow()
  })

  it('resolves boundary angles and compiles gapless rational flattened intervals', () => {
    const value = group(); const cuts = decisions()
    expect(activeMulticamAngle(value, cuts, 1_999, 100).id).toBe('angle-1')
    expect(activeMulticamAngle(value, cuts, 2_000, 100).id).toBe('angle-2')
    const intervals = compileFlattenedMulticamIntervals(value, cuts, 100)
    expect(intervals.map(item => [item.outputStartTick, item.durationTicks, item.mediaId])).toEqual([
      [0, 2_000, 'camera-a'], [2_000, 3_000, 'camera-b'], [5_000, 5_000, 'camera-c'],
    ])
    expect(intervals[1]!.sourceStart).toEqual({ numerator: '2000', denominator: 1 })
    expect(intervals.reduce((sum, item) => sum + item.durationTicks, 0)).toBe(value.durationTicks)
  })

  it('builds deterministic content-bound fingerprint input and rejects missing identities', () => {
    const first = multicamFingerprintInput(group(), decisions(), 100, sources)
    const second = multicamFingerprintInput(group(), decisions(), 100, structuredClone(sources))
    expect(first).toBe(second)
    expect(first).toContain(fingerprint('b'))
    expect(multicamFingerprintInput(group(), decisions().map(item => ({ ...item })), 100, sources)).toBe(first)
    const reordered = JSON.parse(JSON.stringify(group()), (key, value) => {
      if (key === 'rate' && value) return { denominator: value.denominator, numerator: value.numerator }
      return value
    })
    const reorderedRoot = {
      angles: reordered.angles, sync: reordered.sync, audioAngleId: reordered.audioAngleId,
      referenceAngleId: reordered.referenceAngleId, durationTicks: reordered.durationTicks,
      timeBase: reordered.timeBase, name: reordered.name, id: reordered.id, contract: reordered.contract,
    }
    expect(multicamFingerprintInput(reorderedRoot, decisions(), 100, sources)).toBe(first)
    expect(() => multicamFingerprintInput(group(), decisions(), 100, sources.map((item, index) => index ? item : { ...item, contentFingerprint: undefined })))
      .toThrowError(expect.objectContaining<Partial<MulticamError>>({ code: 'invalid_media' }))
  })
})
