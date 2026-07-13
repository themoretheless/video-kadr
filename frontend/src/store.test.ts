import { nextTick } from 'vue'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import * as api from './api'
import {
  state,
  defaultEdit,
  buildEditPayload,
  hasMeaningfulChanges,
  normalizeCrop,
  parseTime,
  tierToCrf,
  history,
  resetHistory,
  undo,
  redo,
  openFromLibrary,
  savePreset,
  applyPreset,
  deletePreset,
  loadPresets,
  loadCapabilities,
  selectedExportUnavailableReason,
  presets,
} from './store'
import type { EditState, VideoInfo } from './types'

vi.mock('./api', () => {
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

/** Put a video and a fresh full-clip edit into the store. */
function setVideo(duration = 10, width = 1280, height = 720): void {
  const v: VideoInfo = {
    id: 'vid',
    url: '/files/sources/vid.mp4',
    filename: 'vid.mp4',
    duration,
    width,
    height,
    title: null,
    sizeBytes: null,
  }
  state.video = v
  const e = defaultEdit()
  e.trimEnd = duration
  e.crop = { x: 0, y: 0, w: width, h: height }
  e.scale = { w: width, h: -2 }
  state.edit = e
}

describe('parseTime', () => {
  it('parses seconds, mm:ss, hh:mm:ss and fractions', () => {
    expect(parseTime('5')).toBe(5)
    expect(parseTime('1:23')).toBe(83)
    expect(parseTime('1:00:00')).toBe(3600)
    expect(parseTime('1:23.5')).toBe(83.5)
  })
  it('returns null for empty or invalid input', () => {
    expect(parseTime('')).toBeNull()
    expect(parseTime('   ')).toBeNull()
    expect(parseTime('abc')).toBeNull()
    expect(parseTime('1:2x')).toBeNull()
  })
})

describe('tierToCrf', () => {
  it('maps quality tiers per format, null otherwise', () => {
    expect(tierToCrf('high', 'mp4')).toBe(18)
    expect(tierToCrf('compact', 'mp4')).toBe(28)
    expect(tierToCrf('high', 'webm')).toBe(28)
    expect(tierToCrf('medium', 'av1')).toBe(34)
    expect(tierToCrf('', 'mp4')).toBeNull()
    expect(tierToCrf('high', 'gif')).toBeNull()
    expect(tierToCrf('weird', 'mp4')).toBeNull()
  })
})

describe('buildEditPayload', () => {
  beforeEach(() => setVideo())

  it('sends a minimal payload for an untouched full clip', () => {
    const p = buildEditPayload()
    expect(p.videoId).toBe('vid')
    expect(p.mute).toBe(false)
    expect(p.speed).toBe(1)
    expect('trim' in p).toBe(false)
    expect('segments' in p).toBe(false)
    expect('crop' in p).toBe(false)
  })

  it('emits a trim when the clip is narrowed', () => {
    state.edit.trimStart = 2
    state.edit.trimEnd = 8
    expect(buildEditPayload().trim).toEqual({ start: 2, end: 8 })
  })

  it('turns a middle cut into keep-segments', () => {
    state.edit.cutEnabled = true
    state.edit.cut = { start: 3, end: 6 }
    const p = buildEditPayload()
    expect(p.segments).toEqual([
      { start: 0, end: 3 },
      { start: 6, end: 10 },
    ])
    expect('trim' in p).toBe(false)
  })

  it('only includes effects that differ from defaults', () => {
    Object.assign(state.edit, {
      rotate: 90,
      volume: 1.5,
      brightness: 0.2,
      filter: 'sepia',
      vignette: true,
    })
    const p = buildEditPayload()
    expect(p.rotate).toBe(90)
    expect(p.volume).toBe(1.5)
    expect(p.brightness).toBe(0.2)
    expect(p.filter).toBe('sepia')
    expect(p.vignette).toBe(true)
    expect('contrast' in p).toBe(false)
    expect('flipH' in p).toBe(false)
  })

  it('maps denoise/sharpen/grain only when set', () => {
    expect('denoise' in buildEditPayload()).toBe(false)
    Object.assign(state.edit, { denoise: true, sharpen: 1.5, grain: 20 })
    const p = buildEditPayload()
    expect(p.denoise).toBe(true)
    expect(p.sharpen).toBe(1.5)
    expect(p.grain).toBe(20)
  })

  it('maps audio normalize/highpass only when set', () => {
    expect('normalizeAudio' in buildEditPayload()).toBe(false)
    Object.assign(state.edit, { normalizeAudio: true, highpass: true })
    const p = buildEditPayload()
    expect(p.normalizeAudio).toBe(true)
    expect(p.highpass).toBe(true)
  })

  it('gates censor by size and maps codec/quality', () => {
    state.edit.censorEnabled = true
    state.edit.censor = { x: 1, y: 1, w: 0, h: 0 } // too small
    expect('censor' in buildEditPayload()).toBe(false)

    state.edit.censor = { x: 1, y: 1, w: 20, h: 20 }
    state.edit.format = 'mp4'
    state.edit.codec = 'h265'
    state.edit.qualityTier = 'high'
    const p = buildEditPayload()
    expect(p.censor).toEqual({ x: 1, y: 1, w: 20, h: 20 })
    expect(p.codec).toBe('h265')
    expect(p.quality).toBe(18)
  })

  it('sanitizes crop values before sending payloads', () => {
    state.edit.cropEnabled = true
    state.edit.crop = {
      x: Number.NaN,
      y: Number.POSITIVE_INFINITY,
      w: Number.NaN,
      h: 0,
    }
    expect(buildEditPayload().crop).toEqual({ x: 0, y: 0, w: 1280, h: 2 })
  })

  it('normalizes crop state after manual numeric input', () => {
    state.edit.crop = {
      x: 9999,
      y: Number.NaN,
      w: Number.NaN,
      h: 9999,
    }
    normalizeCrop()
    expect(state.edit.crop).toEqual({ x: 0, y: 0, w: 1280, h: 720 })
  })

  it('distinguishes an unchanged export from media or output changes', () => {
    expect(hasMeaningfulChanges()).toBe(false)
    state.edit.filter = 'warm'
    expect(hasMeaningfulChanges()).toBe(true)
    state.edit.filter = ''
    state.edit.qualityTier = 'compact'
    expect(hasMeaningfulChanges()).toBe(true)
  })
})

describe('runtime capabilities', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    state.capabilities = null
    state.backendStatus = 'checking'
    setVideo()
  })

  it('distinguishes an older backend from a network outage', async () => {
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

  it('explains why the selected export cannot run', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [
        { id: 'h264', label: 'H.264', available: false, reason: 'libx264 отсутствует' },
      ],
      filters: [],
      hardware: [],
    }
    state.edit.format = 'mp4'
    state.edit.codec = 'h264'

    expect(selectedExportUnavailableReason()).toBe('libx264 отсутствует')
  })
})

describe('history', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setVideo()
    resetHistory()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('flushes a pending edit before redo', async () => {
    state.edit.filter = 'sepia'
    await nextTick()

    undo()
    expect(state.edit.filter).toBe('')
    expect(history.future).toHaveLength(1)

    state.edit.filter = 'warm'
    await nextTick()
    redo()

    expect(state.edit.filter).toBe('warm')
    expect(history.future).toHaveLength(0)
  })
})

describe('project restore autosave', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    setVideo()
    resetHistory()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('does not save default edit while restore is in flight', async () => {
    let resolveProject: (value: Awaited<ReturnType<typeof api.getProjectByVideo>>) => void = () => {}
    vi.mocked(api.getProjectByVideo).mockReturnValue(
      new Promise((resolve) => {
        resolveProject = resolve
      }) as ReturnType<typeof api.getProjectByVideo>,
    )

    openFromLibrary({
      id: 'saved',
      kind: 'source',
      filename: 'saved.mp4',
      url: '/files/sources/saved.mp4',
      duration: 20,
      width: 640,
      height: 360,
      createdAt: 1,
    })

    await nextTick()
    await vi.advanceTimersByTimeAsync(1500)
    expect(api.saveProject).not.toHaveBeenCalled()

    resolveProject({
      id: 'p1',
      name: 'saved',
      videoId: 'saved',
      video: state.video!,
      edit: { filter: 'sepia' } satisfies Partial<EditState>,
      createdAt: 1,
      updatedAt: 2,
    })
    await Promise.resolve()
    await nextTick()
    await vi.advanceTimersByTimeAsync(1500)

    expect(state.edit.filter).toBe('sepia')
    expect(api.saveProject).not.toHaveBeenCalled()
  })
})

describe('effect presets', () => {
  beforeEach(() => {
    localStorage.clear()
    presets.list = []
    setVideo()
  })

  it('captures only reusable effect keys, not clip geometry', () => {
    Object.assign(state.edit, {
      filter: 'sepia',
      speed: 1.5,
      trimStart: 3,
      cropEnabled: true,
      format: 'webm',
      qualityTier: 'compact',
    })
    state.edit.crop = { x: 1, y: 2, w: 3, h: 4 }
    savePreset('look')
    expect(presets.list).toHaveLength(1)
    const p = presets.list[0]
    expect(p.edit.filter).toBe('sepia')
    expect(p.edit.speed).toBe(1.5)
    expect('trimStart' in p.edit).toBe(false)
    expect('crop' in p.edit).toBe(false)
    expect('format' in p.edit).toBe(false)
    expect('qualityTier' in p.edit).toBe(false)
    expect(JSON.parse(localStorage.getItem('ve_presets')!)).toHaveLength(1)
  })

  it('applies a preset onto the current edit', () => {
    state.edit.filter = ''
    state.edit.speed = 1
    applyPreset({ name: 'x', edit: { filter: 'warm', speed: 2 } })
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.speed).toBe(2)
  })

  it('does not let legacy look presets change export settings', () => {
    state.edit.format = 'mp4'
    state.edit.qualityTier = 'high'
    applyPreset({
      name: 'legacy',
      edit: { filter: 'warm', format: 'webm', qualityTier: 'compact' },
    })
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.format).toBe('mp4')
    expect(state.edit.qualityTier).toBe('high')
  })

  it('deletes and reloads presets from storage', () => {
    savePreset('a')
    savePreset('b')
    deletePreset('a')
    expect(presets.list.map((p) => p.name)).toEqual(['b'])
    presets.list = []
    loadPresets()
    expect(presets.list.map((p) => p.name)).toEqual(['b'])
  })
})
