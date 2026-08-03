import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  state: { playerTime: 0, library: [] as Array<Record<string, unknown>> },
  timelineState: { document: null as Record<string, unknown> | null, error: '' },
  execute: vi.fn(),
}))

vi.mock('./store', () => ({
  state: mocks.state,
  timelineState: mocks.timelineState,
  executeTimelineCommand: mocks.execute,
}))

import * as api from './api'
import {
  createMulticam,
  attachedMulticamProgramAt,
  multicamState,
  projectMulticamSources,
  resyncMulticam,
  selectMulticamGroup,
} from './multicam'

function document() {
  return {
    activeSequenceId: 'sequence',
    primaryMediaId: 'cam-a',
    media: [
      { id: 'cam-a', kind: 'video', assetRef: 'asset-a', metadata: { filename: 'A.mp4', duration: 10, fps: 25 } },
      { id: 'cam-b', kind: 'video', assetRef: 'asset-b', metadata: { filename: 'B.mp4', duration: 12, fps: 25 } },
    ],
    sequences: [{ id: 'sequence', settings: { timeBase: 1_000, frameRate: 25 }, tracks: [{ id: 'video', kind: 'video', clips: [{ id: 'clip-a', mediaId: 'cam-a', timelineStartTick: 0, durationTicks: 10_000, sourceInTick: 0, sourceOutTick: 10_000, effects: [] }] }] }],
    multicamGroups: [] as Array<Record<string, unknown>>,
  }
}

describe('multicam facade sync and reopen', () => {
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.timelineState.document = document()
    mocks.state.library = [
      { id: 'asset-a', filename: 'A.mp4', availability: 'ready' },
      { id: 'asset-b', filename: 'B.mp4', availability: 'ready' },
      { id: 'global-only', filename: 'Not in project.mp4', availability: 'ready' },
    ]
    mocks.execute.mockImplementation(command => {
      const commands = command.kind === 'batch' ? command.commands : [command]
      for (const item of commands) {
        if (item.kind === 'set_multicam_groups') (mocks.timelineState.document!.multicamGroups as unknown[]) = item.groups
      }
      return true
    })
    selectMulticamGroup(null)
  })

  it('scopes sources to the active project and commits marker sync atomically', async () => {
    expect(projectMulticamSources.value.map(item => item.id)).toEqual(['cam-a', 'cam-b'])
    await createMulticam({
      name: 'Interview', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'marker',
      markerAnchors: { 'cam-a': 1_000, 'cam-b': 2_000 },
    })
    expect(mocks.execute).toHaveBeenCalledOnce()
    const group = (mocks.timelineState.document!.multicamGroups as Array<Record<string, unknown>>)[0]!
    expect(group.sync).toMatchObject({ method: 'marker', algorithmVersion: 'marker-anchor-v1' })
    expect((group.angles as Array<Record<string, unknown>>).map(angle => angle.sourceOriginTick)).toEqual([1_000, 2_000])
    expect(group.durationTicks).toBe(9_000)
  })

  it('reopens the first persisted group and validates explicit selection', async () => {
    await createMulticam({
      name: 'Saved', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'marker',
      markerAnchors: { 'cam-a': 0, 'cam-b': 0 },
    })
    const id = ((mocks.timelineState.document!.multicamGroups as Array<Record<string, unknown>>)[0]!.id as string)
    selectMulticamGroup(null)
    expect(multicamState.activeGroupId).toBe(id)
    expect(() => selectMulticamGroup('missing')).toThrow(/не найдена/)
  })

  it('resyncs in one command and leaves the prior group exact on validation failure', async () => {
    await createMulticam({
      name: 'Saved', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'marker',
      markerAnchors: { 'cam-a': 0, 'cam-b': 0 },
    })
    const id = ((mocks.timelineState.document!.multicamGroups as Array<Record<string, unknown>>)[0]!.id as string)
    const before = structuredClone((mocks.timelineState.document!.multicamGroups as unknown[])[0])
    mocks.execute.mockClear()
    await expect(resyncMulticam(id, {
      name: 'Saved', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'timecode',
      timecodeAnchors: { 'cam-a': { startFrame: 0, rate: { numerator: 25, denominator: 1 }, dropFrame: false } },
    })).rejects.toThrow(/Таймкод не указан/)
    expect(mocks.execute).not.toHaveBeenCalled()
    expect((mocks.timelineState.document!.multicamGroups as unknown[])[0]).toEqual(before)
  })

  it('maps output seeks across cuts to each angle source clock', () => {
    const doc = mocks.timelineState.document as ReturnType<typeof document>
    doc.multicamGroups = [{
      contract: 'multicam-v1', id: 'group', name: 'Group', timeBase: 1_000, durationTicks: 9_000,
      referenceAngleId: 'angle-a', audioAngleId: 'angle-a', sync: { method: 'marker', algorithmVersion: 'test-v1' },
      angles: [
        { id: 'angle-a', mediaId: 'cam-a', label: 'A', sourceOriginTick: 500, rate: { numerator: 1, denominator: 1 }, enabled: true },
        { id: 'angle-b', mediaId: 'cam-b', label: 'B', sourceOriginTick: 2_000, rate: { numerator: 2, denominator: 1 }, enabled: true },
      ],
      decisions: [{ id: 'cut-a', offsetTick: 0, angleId: 'angle-a' }, { id: 'cut-b', offsetTick: 3_000, angleId: 'angle-b' }],
    }]
    const clip = doc.sequences[0]!.tracks[0]!.clips[0]!
    Object.assign(clip, { timelineStartTick: 2_000, sourceInTick: 1_000, sourceOutTick: 8_000, durationTicks: 7_000, multicamGroupId: 'group' })

    expect(attachedMulticamProgramAt(1.999)).toBeNull()
    expect(attachedMulticamProgramAt(2)).toMatchObject({ angleId: 'angle-a', sourceSeconds: 1.5, rate: 1 })
    expect(attachedMulticamProgramAt(4)).toMatchObject({ angleId: 'angle-b', sourceSeconds: 8, rate: 2 })
    const loopEnd = attachedMulticamProgramAt(8.999)
    const loopStart = attachedMulticamProgramAt(2)
    expect(loopEnd).toMatchObject({ angleId: 'angle-b', rate: 2 })
    expect(loopStart).toMatchObject({ angleId: 'angle-a', sourceSeconds: 1.5, rate: 1 })
    expect(attachedMulticamProgramAt(9)).toBeNull()
  })

  it('delegates long sources to bounded FFmpeg envelope extraction', async () => {
    const media = (mocks.timelineState.document!.media as Array<{ metadata: { duration: number } }>)[0]!
    media.metadata.duration = 3_600
    let seed = 0x12345678
    const samples = new Float32Array(1_000)
    for (let index = 0; index < samples.length; index++) {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
      samples[index] = seed / 0xffff_ffff
    }
    const extract = vi.spyOn(api, 'extractAudioSyncEnvelope').mockResolvedValue({ samples, secondsPerSample: .02 })
    await createMulticam({
      name: 'Long', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'audio',
    })
    expect(extract).toHaveBeenCalledTimes(2)
    expect(extract).toHaveBeenNthCalledWith(1, 'asset-a', undefined)
  })

  it('correlates extracted envelopes on one fixed 50 Hz grid', async () => {
    let seed = 0x12345678
    const pattern = Array.from({ length: 600 }, () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
      return seed / 0xffff_ffff
    })
    const makeEnvelope = (seconds: number, shiftBuckets: number) => {
      const data = new Float32Array(seconds * 50)
      for (let bucket = 0; bucket < data.length - shiftBuckets; bucket++) {
        const value = pattern[bucket]!
        data[bucket + shiftBuckets] = value
      }
      return { samples: data, secondsPerSample: .02 }
    }
    const envelopes = [makeEnvelope(10, 0), makeEnvelope(12, 5)]
    vi.spyOn(api, 'extractAudioSyncEnvelope').mockImplementation(async () => envelopes.shift()!)
    await createMulticam({ name: 'Grid', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'audio' })
    const group = (mocks.timelineState.document!.multicamGroups as Array<Record<string, unknown>>)[0]!
    const origins = (group.angles as Array<{ sourceOriginTick: number }>).map(angle => angle.sourceOriginTick)
    expect(Math.abs(origins[1]! - origins[0]!)).toBe(100)
  })

  it('surfaces bounded extractor failures without committing', async () => {
    vi.spyOn(api, 'extractAudioSyncEnvelope').mockRejectedValue(new Error('MEMFS fallback ограничен 64 МБ'))
    await expect(createMulticam({
      name: 'Huge PCM', mediaIds: ['cam-a', 'cam-b'], referenceMediaId: 'cam-a', syncMode: 'audio',
    })).rejects.toThrow(/MEMFS fallback/)
    expect(mocks.execute).not.toHaveBeenCalled()
  })
})
