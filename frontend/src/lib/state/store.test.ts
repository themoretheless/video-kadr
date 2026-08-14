import { tick } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api'
import {
  activateTimeline,
  beginEditTransaction,
  buildEditPayload,
  defaultEdit,
  deleteTimelineSegment,
  duplicateTimelineSegment,
  endEditTransaction,
  hasMeaningfulChanges,
  history,
  identityCurves,
  initStateEffects,
  loadCapabilities,
  moveTimelineSegment,
  normalizeCrop,
  parseTime,
  presets,
  redo,
  resetHistory,
  sanitizeCurve,
  sanitizeEditState,
  selectedExportUnavailableReason,
  splitTimelineSegment,
  state,
  tierToCrf,
  undo,
  updateTimelineSegmentRange,
} from './store.svelte.js'
import type { VideoInfo } from '../types'

const TEST_LUT_ID = '11111111-1111-4111-8111-111111111111'

vi.mock('../api', () => {
  class ApiError extends Error {
    constructor(
      message: string,
      readonly status: number,
      readonly code?: string,
    ) {
      super(message)
    }
  }
  class BackendUnavailableError extends Error {}

  return {
    ApiError,
    BackendUnavailableError,
    importUrl: vi.fn(),
    uploadFile: vi.fn(),
    uploadLut: vi.fn(),
    getLut: vi.fn((id: string) =>
      Promise.resolve({ id, name: 'Stored LUT', cubeSize: 17, sizeBytes: 128 }),
    ),
    edit: vi.fn(),
    pollJob: vi.fn(),
    getLibrary: vi.fn(() => Promise.resolve([])),
    deleteLibraryItem: vi.fn(),
    saveProject: vi.fn(() => Promise.resolve({})),
    getProjectByVideo: vi.fn(() => Promise.resolve(null)),
    getProjects: vi.fn(() => Promise.resolve([])),
    deleteProject: vi.fn(),
    cancelJob: vi.fn(),
    getCapabilities: vi.fn(() => Promise.resolve(null)),
  }
})

function setVideo(duration = 10, width = 1280, height = 720): void {
  const video: VideoInfo = {
    id: 'vid',
    url: '/files/sources/vid.mp4',
    filename: 'vid.mp4',
    duration,
    width,
    height,
    title: null,
    sizeBytes: null,
  }
  state.video = video
  const edit = defaultEdit()
  edit.trimEnd = duration
  edit.crop = { x: 0, y: 0, w: width, h: height }
  edit.scale = { w: width, h: -2 }
  state.edit = edit
}

beforeEach(() => {
  vi.clearAllMocks()
  setVideo(12, 1920, 1080)
  state.capabilities = null
  state.backendStatus = 'checking'
  state.seekTimelineSegmentId = null
  presets.list = []
  resetHistory()
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('pure edit helpers', () => {
  it('parses time inputs and maps format-specific quality tiers', () => {
    expect(parseTime('5')).toBe(5)
    expect(parseTime('1:23.5')).toBe(83.5)
    expect(parseTime('1:00:00')).toBe(3600)
    expect(parseTime('')).toBeNull()
    expect(parseTime('1:2x')).toBeNull()

    expect(tierToCrf('high', 'mp4')).toBe(18)
    expect(tierToCrf('compact', 'mp4')).toBe(28)
    expect(tierToCrf('high', 'webm')).toBe(28)
    expect(tierToCrf('high', 'gif')).toBeNull()
  })

  it('deep-clones identity curves for independent edits', () => {
    const first = defaultEdit()
    const second = defaultEdit()
    first.curves.red[0]!.y = 42
    first.curves.master.push({ x: 128, y: 140 })

    expect(second.curves).toEqual(identityCurves())
    expect(second.curves.red).not.toBe(first.curves.red)
  })

  it('canonicalizes persisted curves and LUT metadata', () => {
    expect(
      sanitizeCurve([
        { x: 200.2, y: -1 },
        { x: 50.7, y: 300 },
        { x: 51, y: 90 },
        { x: Number.NaN, y: 1 },
      ]),
    ).toEqual([
      { x: 0, y: 0 },
      { x: 51, y: 90 },
      { x: 200, y: 0 },
      { x: 255, y: 255 },
    ])

    const edit = sanitizeEditState({
      lutId: `  ${TEST_LUT_ID}  `,
      lutName: ' Look ',
      lutSize: 999,
      lutIntensity: Number.POSITIVE_INFINITY,
      curves: { master: [{ x: 128, y: 80 }] },
    })
    expect(edit).toMatchObject({
      lutId: TEST_LUT_ID,
      lutName: 'Look',
      lutSize: 65,
      lutIntensity: 1,
    })
    expect(edit.curves.master).toEqual([
      { x: 0, y: 0 },
      { x: 128, y: 80 },
      { x: 255, y: 255 },
    ])
  })

  it('canonicalizes persisted chroma key controls', () => {
    const edit = sanitizeEditState({
      chromaKeyEnabled: true,
      chromaKeyColor: ' 33AAff ',
      chromaKeySimilarity: 9,
      chromaKeyBlend: -1,
      chromaKeySpill: Number.NaN,
    })

    expect(edit).toMatchObject({
      chromaKeyEnabled: true,
      chromaKeyColor: '#33aaff',
      chromaKeySimilarity: 1,
      chromaKeyBlend: 0,
      chromaKeySpill: 0,
    })
    expect(sanitizeEditState({ chromaKeyColor: 'green' }).chromaKeyColor).toBe('#00ff00')
  })
})

describe('edit API payload parity', () => {
  beforeEach(() => setVideo())

  it('sends the minimal untouched full-clip payload', () => {
    const payload = buildEditPayload()
    expect(payload).toMatchObject({ videoId: 'vid', mute: false, speed: 1 })
    expect('trim' in payload).toBe(false)
    expect('segments' in payload).toBe(false)
    expect('crop' in payload).toBe(false)
    expect(hasMeaningfulChanges()).toBe(false)
  })

  it('turns a middle cut into keep-segments and keeps export format parity', () => {
    state.edit.cutEnabled = true
    state.edit.cut = { start: 3, end: 6 }
    expect(buildEditPayload().segments).toEqual([
      { start: 0, end: 3 },
      { start: 6, end: 10 },
    ])

    state.edit.format = 'prores'
    expect(buildEditPayload().segments).toEqual([
      { start: 0, end: 3 },
      { start: 6, end: 10 },
    ])
  })

  it('includes only changed effects, codec and quality fields', () => {
    Object.assign(state.edit, {
      rotate: 90,
      volume: 1.5,
      brightness: 0.2,
      filter: 'sepia',
      vignette: true,
      codec: 'h265',
      qualityTier: 'high',
    })
    const payload = buildEditPayload()
    expect(payload).toMatchObject({
      rotate: 90,
      volume: 1.5,
      brightness: 0.2,
      filter: 'sepia',
      vignette: true,
      codec: 'h265',
      quality: 18,
    })
    expect('contrast' in payload).toBe(false)
    expect('flipH' in payload).toBe(false)
  })

  it('emits chroma key only when enabled and sanitizes every control', () => {
    expect('chromaKey' in buildEditPayload()).toBe(false)
    Object.assign(state.edit, {
      chromaKeyEnabled: true,
      chromaKeyColor: '3366CC',
      chromaKeySimilarity: 2,
      chromaKeyBlend: -0.2,
      chromaKeySpill: 0.7,
    })

    expect(buildEditPayload().chromaKey).toEqual({
      keyColor: '#3366cc',
      similarity: 1,
      blend: 0,
      spillSuppression: 0.7,
    })
    expect(hasMeaningfulChanges()).toBe(true)
  })

  it('emits sanitized LUT and curves without UI metadata', () => {
    state.edit.lutId = `  ${TEST_LUT_ID}  `
    state.edit.lutName = 'Film look'
    state.edit.lutSize = 33
    state.edit.lutIntensity = 3
    state.edit.curves.red = [
      { x: 255, y: 260 },
      { x: 128.4, y: 999 },
      { x: 0, y: -10 },
      { x: 128.2, y: 44 },
    ]

    const payload = buildEditPayload()
    expect(payload.lut).toEqual({ id: TEST_LUT_ID, intensity: 1 })
    expect(payload.curves).toMatchObject({
      red: [
        { x: 0, y: 0 },
        { x: 128 / 255, y: 44 / 255 },
        { x: 1, y: 1 },
      ],
    })
    expect('lutName' in payload).toBe(false)
    expect('lutSize' in payload).toBe(false)
  })

  it('normalizes unsafe crop input before it reaches the backend', () => {
    state.edit.cropEnabled = true
    state.edit.crop = {
      x: Number.NaN,
      y: Number.POSITIVE_INFINITY,
      w: Number.NaN,
      h: 0,
    }
    expect(buildEditPayload().crop).toEqual({ x: 0, y: 0, w: 1280, h: 2 })

    state.edit.crop = { x: 9999, y: Number.NaN, w: Number.NaN, h: 9999 }
    normalizeCrop()
    expect(state.edit.crop).toEqual({ x: 0, y: 0, w: 1280, h: 720 })
  })
})

describe('runtime capabilities', () => {
  it('distinguishes an older backend from an outage', async () => {
    vi.mocked(api.getCapabilities).mockRejectedValueOnce(new api.ApiError('not found', 404))
    await loadCapabilities()
    expect(state.backendStatus).toBe('online')
    expect(state.capabilities).toBeNull()

    vi.mocked(api.getCapabilities).mockRejectedValueOnce(new api.ApiError('proxy failed', 503))
    await loadCapabilities()
    expect(state.backendStatus).toBe('offline')

    vi.mocked(api.getCapabilities).mockRejectedValueOnce(new api.BackendUnavailableError())
    await loadCapabilities()
    expect(state.backendStatus).toBe('offline')
  })

  it('reports an unavailable selected codec', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: false, reason: 'libx264 отсутствует' }],
      filters: [],
      hardware: [],
    }
    state.edit.codec = 'h264'

    expect(selectedExportUnavailableReason()).toBe('libx264 отсутствует')
  })

  it('fails closed for active color grades and requires blend support only below 100%', () => {
    state.edit.lutId = TEST_LUT_ID
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')

    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [{ id: 'lut3d', label: '3D LUT', available: true }],
      hardware: [],
    }
    expect(selectedExportUnavailableReason()).toBeNull()

    state.edit.lutIntensity = 0.5
    expect(selectedExportUnavailableReason()).toContain('Частичная интенсивность')
    state.capabilities.filters.push({
      id: 'lut-intensity',
      label: 'Частичная интенсивность 3D LUT',
      available: true,
    })
    expect(selectedExportUnavailableReason()).toBeNull()
  })

  it('fails closed for chroma key and requires despill only when requested', () => {
    state.edit.chromaKeyEnabled = true
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')

    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [{ id: 'chroma-key', label: 'Chroma key', available: true }],
      hardware: [],
    }
    expect(selectedExportUnavailableReason()).toBeNull()

    state.edit.chromaKeySpill = 0.4
    expect(selectedExportUnavailableReason()).toContain('chroma spill')
    state.capabilities.filters.push({
      id: 'chroma-spill',
      label: 'Подавление chroma spill',
      available: true,
    })
    expect(selectedExportUnavailableReason()).toBeNull()
  })
})

describe('Svelte app state timeline parity', () => {
  it('preserves playback order and duplicate source ranges in the wire payload', () => {
    state.edit.timelineEnabled = true
    state.edit.timelineSegments = [
      { id: 'blue', start: 6, end: 9 },
      { id: 'red-1', start: 0, end: 3 },
      { id: 'red-2', start: 0, end: 3 },
      { id: 'green', start: 3, end: 6 },
    ]

    expect(buildEditPayload().segments).toEqual([
      { start: 6, end: 9 },
      { start: 0, end: 3 },
      { start: 0, end: 3 },
      { start: 3, end: 6 },
    ])
  })

  it('supports activation, split, duplicate, reorder and delete', () => {
    const first = activateTimeline()
    expect(first).toBe('segment-1')

    const right = splitTimelineSegment(first!, 5)
    expect(right).toBe('segment-2')
    const duplicate = duplicateTimelineSegment(right!)
    expect(duplicate).toBe('segment-3')
    expect(moveTimelineSegment(duplicate!, -1)).toBe(true)
    expect(state.edit.timelineSegments.map(({ id }) => id)).toEqual([
      'segment-1',
      'segment-3',
      'segment-2',
    ])

    expect(deleteTimelineSegment(duplicate!)).toBe('segment-2')
    expect(state.edit.timelineSegments.map(({ id }) => id)).toEqual(['segment-1', 'segment-2'])
  })

  it('clamps exact range updates to the source bounds and minimum duration', () => {
    const id = activateTimeline()!
    expect(updateTimelineSegmentRange(id, { start: -10, end: 20 })).toBe(false)
    expect(state.edit.timelineSegments[0]).toEqual({ id, start: 0, end: 12 })
    expect(updateTimelineSegmentRange(id, { start: 11.5 })).toBe(true)
    expect(state.edit.timelineSegments[0]).toEqual({ id, start: 11.5, end: 12 })
  })

  it('keeps explicit edit transactions undoable and redoable', () => {
    beginEditTransaction('speed')
    state.edit.speed = 1.5
    endEditTransaction()

    expect(history.past).toHaveLength(1)
    undo()
    expect(state.edit.speed).toBe(1)
    redo()
    expect(state.edit.speed).toBe(1.5)
  })

  it('tracks direct nested UI mutations through the Svelte deep effect', async () => {
    vi.useFakeTimers()
    state.video = null
    const dispose = initStateEffects()
    try {
      await tick()
      state.edit.brightness = 0.25
      await tick()
      vi.advanceTimersByTime(351)

      expect(history.past).toHaveLength(1)
      undo()
      expect(state.edit.brightness).toBe(0)
    } finally {
      dispose()
      vi.useRealTimers()
    }
  })
})
