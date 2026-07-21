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
  beginEditTransaction,
  endEditTransaction,
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
  clearLut,
  doUploadLut,
  identityCurves,
  resetColor,
  sampleCurvePchip,
  sanitizeCurve,
  sanitizeEditState,
} from './store'
import type { EditState, VideoInfo } from './types'

const TEST_LUT_ID = '11111111-1111-4111-8111-111111111111'
const SECOND_LUT_ID = '22222222-2222-4222-8222-222222222222'

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

function memoryStorage(): Storage {
  const values = new Map<string, string>()
  return {
    get length() {
      return values.size
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, String(value)),
  }
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

  it.each(['av1', 'prores'])('keeps cut segments for %s exports', (format) => {
    state.edit.format = format
    state.edit.cutEnabled = true
    state.edit.cut = { start: 3, end: 6 }

    expect(buildEditPayload().segments).toEqual([
      { start: 0, end: 3 },
      { start: 6, end: 10 },
    ])
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

  it('deep-clones identity curves for each edit', () => {
    const first = defaultEdit()
    const second = defaultEdit()
    first.curves.red[0].y = 42
    first.curves.master.push({ x: 128, y: 140 })

    expect(second.curves).toEqual(identityCurves())
    expect(second.curves.red).not.toBe(first.curves.red)
  })

  it('emits a sanitized deterministic LUT and curves payload', () => {
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
    expect(payload.curves).toEqual({
      master: [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      red: [
        { x: 0, y: 0 },
        { x: 128 / 255, y: 44 / 255 },
        { x: 1, y: 1 },
      ],
      green: [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      blue: [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
    })
    // UI-only metadata is never part of the edit request.
    expect('lutName' in payload).toBe(false)
    expect('lutSize' in payload).toBe(false)
  })

  it('omits identity curves and a zero-intensity LUT', () => {
    state.edit.lutId = TEST_LUT_ID
    state.edit.lutIntensity = -2
    state.edit.curves = identityCurves()

    const payload = buildEditPayload()
    expect('lut' in payload).toBe(false)
    expect('curves' in payload).toBe(false)
    expect(hasMeaningfulChanges()).toBe(false)
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

describe('colour state sanitation', () => {
  it('canonicalizes curve points and persisted LUT metadata', () => {
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
    expect(edit.lutId).toBe(TEST_LUT_ID)
    expect(edit.lutName).toBe('Look')
    expect(edit.lutSize).toBe(65)
    expect(edit.lutIntensity).toBe(1)
    expect(edit.curves.master).toEqual([
      { x: 0, y: 0 },
      { x: 128, y: 80 },
      { x: 255, y: 255 },
    ])
    expect(edit.curves.red).toEqual(identityCurves().red)
  })

  it('resets overlong curves and rejects non-UUID persisted LUT ids', () => {
    const overlong = Array.from({ length: 17 }, (_, index) => ({
      x: Math.round((index / 16) * 255),
      y: index * 8,
    }))
    expect(sanitizeCurve(overlong)).toEqual(identityCurves().master)
    expect(sanitizeEditState({ lutId: '../look.cube', lutName: 'Unsafe' }).lutId).toBeNull()
  })

  it('samples the same shape-preserving cubic curve shown for PCHIP export', () => {
    const identity = sampleCurvePchip(identityCurves().master)
    expect(identity).toHaveLength(256)
    expect(identity[64]!.y).toBeCloseTo(64)
    expect(identity[255]).toEqual({ x: 255, y: 255 })

    const lifted = sampleCurvePchip([
      { x: 0, y: 0 },
      { x: 128, y: 200 },
      { x: 255, y: 255 },
    ])
    expect(lifted[64]!.y).toBeGreaterThan(100)
    expect(lifted[128]!.y).toBeCloseTo(200)
  })
})

describe('LUT store actions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    setVideo()
    resetHistory()
    state.lutUploading = false
    state.lutUploadError = ''
  })

  it('uploads and attaches validated LUT metadata', async () => {
    vi.mocked(api.uploadLut).mockResolvedValue({
      id: TEST_LUT_ID,
      name: 'Cinema',
      cubeSize: 33,
      sizeBytes: 1234,
      sha256: 'abc',
    })
    const file = new File(['TITLE Cinema'], 'cinema.CUBE', { type: 'text/plain' })

    await expect(doUploadLut(file)).resolves.toMatchObject({ id: TEST_LUT_ID })
    expect(api.uploadLut).toHaveBeenCalledWith(file)
    expect(state.edit).toMatchObject({
      lutId: TEST_LUT_ID,
      lutName: 'Cinema',
      lutSize: 33,
      lutIntensity: 1,
    })
    expect(state.lutUploading).toBe(false)
    expect(state.lutUploadError).toBe('')
  })

  it('rejects non-cube files and preserves an existing LUT on upload failure', async () => {
    state.edit.lutId = 'old'
    state.edit.lutName = 'Old'
    expect(await doUploadLut(new File(['x'], 'look.txt'))).toBeNull()
    expect(api.uploadLut).not.toHaveBeenCalled()

    vi.mocked(api.uploadLut).mockRejectedValueOnce(new Error('invalid cube'))
    expect(await doUploadLut(new File(['x'], 'new.cube'))).toBeNull()
    expect(state.edit.lutId).toBe('old')
    expect(state.edit.lutName).toBe('Old')
    expect(state.lutUploadError).toBe('invalid cube')
  })

  it('does not attach a completed LUT upload to a clip opened later', async () => {
    let resolveUpload: (asset: Awaited<ReturnType<typeof api.uploadLut>>) => void = () => {}
    vi.mocked(api.uploadLut).mockReturnValueOnce(
      new Promise((resolve) => {
        resolveUpload = resolve
      }) as ReturnType<typeof api.uploadLut>,
    )
    const uploadPromise = doUploadLut(new File(['cube'], 'slow.cube'))

    const otherVideo = { ...state.video!, id: 'other-video', filename: 'other.mp4' }
    state.video = otherVideo
    state.edit = defaultEdit()
    state.edit.trimEnd = otherVideo.duration
    resolveUpload({
      id: TEST_LUT_ID,
      name: 'Slow LUT',
      cubeSize: 17,
      sizeBytes: 100,
      sha256: 'abc',
    })

    await uploadPromise
    expect(state.video.id).toBe('other-video')
    expect(state.edit.lutId).toBeNull()
  })

  it('clears LUT alone and resets the complete colour stack', () => {
    Object.assign(state.edit, {
      brightness: 0.4,
      filter: 'warm',
      lutId: TEST_LUT_ID,
      lutName: 'Look',
      lutSize: 17,
      lutIntensity: 0.5,
      vignette: true,
      denoise: true,
      sharpen: 2,
      grain: 10,
    })
    state.edit.curves.blue = [
      { x: 0, y: 10 },
      { x: 255, y: 240 },
    ]

    clearLut()
    expect(state.edit.lutId).toBeNull()
    expect(state.edit.brightness).toBe(0.4)

    resetColor()
    expect(state.edit).toMatchObject({
      brightness: 0,
      contrast: 1,
      saturation: 1,
      filter: '',
      lutId: null,
      lutName: '',
      lutSize: null,
      lutIntensity: 1,
      vignette: false,
      denoise: false,
      sharpen: 0,
      grain: 0,
    })
    expect(state.edit.curves).toEqual(identityCurves())
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

  it('blocks active grades when the server omits or disables their capabilities', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [],
      hardware: [],
    }
    state.edit.lutId = TEST_LUT_ID
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')

    state.capabilities.filters = [
      { id: 'lut3d', label: '3D LUT', available: true },
      { id: 'custom-curves', label: 'Кривые', available: false, reason: 'нет curves' },
    ]
    state.edit.curves.red = [
      { x: 0, y: 0 },
      { x: 255, y: 240 },
    ]
    expect(selectedExportUnavailableReason()).toBe('нет curves')
  })

  it('fails closed for an active grade when the capability manifest is unavailable', () => {
    state.capabilities = null
    state.edit.lutId = TEST_LUT_ID
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')

    state.edit.lutId = null
    state.edit.curves.master = [
      { x: 0, y: 0 },
      { x: 255, y: 240 },
    ]
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')
  })

  it('requires blend capability only for partial LUT intensity', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [{ id: 'lut3d', label: '3D LUT', available: true }],
      hardware: [],
    }
    state.edit.lutId = TEST_LUT_ID
    state.edit.lutIntensity = 1
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

  it('coalesces pointer movement into one field-level command', async () => {
    beginEditTransaction('crop-drag')
    state.edit.crop = { x: 10, y: 0, w: 100, h: 100 }
    await nextTick()
    state.edit.crop = { x: 30, y: 20, w: 80, h: 70 }
    await nextTick()
    endEditTransaction()

    expect(history.past).toHaveLength(1)
    expect(history.past[0].changedKeys).toEqual(['crop'])
    undo()
    expect(state.edit.crop).toEqual({ x: 0, y: 0, w: 1280, h: 720 })
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

    const savedCurves = identityCurves()
    savedCurves.green = [
      { x: 0, y: 4 },
      { x: 255, y: 250 },
    ]
    resolveProject({
      id: 'p1',
      name: 'saved',
      videoId: 'saved',
      video: state.video!,
      edit: {
        filter: 'sepia',
        lutId: TEST_LUT_ID,
        lutName: 'Saved look',
        lutSize: 17,
        lutIntensity: 0.6,
        curves: savedCurves,
      } satisfies Partial<EditState>,
      createdAt: 1,
      updatedAt: 2,
    })
    await Promise.resolve()
    await nextTick()
    await vi.advanceTimersByTimeAsync(1500)

    expect(state.edit.filter).toBe('sepia')
    expect(state.edit.lutId).toBe(TEST_LUT_ID)
    expect(state.edit.lutIntensity).toBe(0.6)
    expect(state.edit.curves.green).toEqual(savedCurves.green)
    expect(api.saveProject).not.toHaveBeenCalled()
  })

  it('detaches only a missing LUT while restoring the remaining colour grade', async () => {
    const curves = identityCurves()
    curves.blue = [
      { x: 0, y: 5 },
      { x: 255, y: 245 },
    ]
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce({
      id: 'p2',
      name: 'saved',
      videoId: 'saved-missing-lut',
      video: state.video!,
      edit: { lutId: TEST_LUT_ID, lutName: 'Gone', curves },
      createdAt: 1,
      updatedAt: 2,
    })
    vi.mocked(api.getLut).mockRejectedValueOnce(new api.ApiError('LUT не найден', 404))

    openFromLibrary({
      id: 'saved-missing-lut',
      kind: 'source',
      filename: 'saved.mp4',
      url: '/files/sources/saved.mp4',
      duration: 20,
      width: 640,
      height: 360,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()

    expect(state.edit.lutId).toBeNull()
    expect(state.edit.curves.blue).toEqual(curves.blue)
  })

  it('keeps newer user edits while restored LUT metadata is still loading', async () => {
    let resolveLut: (asset: Awaited<ReturnType<typeof api.getLut>>) => void = () => {}
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce({
      id: 'p3',
      name: 'saved',
      videoId: 'saved-race',
      video: state.video!,
      edit: { filter: 'sepia', lutId: TEST_LUT_ID, lutName: 'Saved look' },
      createdAt: 1,
      updatedAt: 2,
    })
    vi.mocked(api.getLut).mockReturnValueOnce(
      new Promise((resolve) => {
        resolveLut = resolve
      }) as ReturnType<typeof api.getLut>,
    )

    openFromLibrary({
      id: 'saved-race',
      kind: 'source',
      filename: 'saved.mp4',
      url: '/files/sources/saved.mp4',
      duration: 20,
      width: 640,
      height: 360,
      createdAt: 1,
    })
    await Promise.resolve()
    state.edit.filter = 'warm'
    resolveLut({ id: TEST_LUT_ID, name: 'Saved look', cubeSize: 17, sizeBytes: 100 })
    await Promise.resolve()
    await nextTick()

    expect(state.edit.filter).toBe('warm')
    expect(state.edit.lutId).toBeNull()
    await vi.advanceTimersByTimeAsync(1000)
    expect(api.saveProject).toHaveBeenCalledWith(
      expect.objectContaining({ edit: expect.objectContaining({ filter: 'warm' }) }),
    )
  })
})

describe('effect presets', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('localStorage', memoryStorage())
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
    state.edit.lutId = TEST_LUT_ID
    state.edit.lutName = 'Look'
    state.edit.lutSize = 33
    state.edit.lutIntensity = 0.75
    state.edit.curves.red = [
      { x: 0, y: 0 },
      { x: 128, y: 150 },
      { x: 255, y: 255 },
    ]
    savePreset('look')
    expect(presets.list).toHaveLength(1)
    const p = presets.list[0]
    expect(p.edit.filter).toBe('sepia')
    expect(p.edit.speed).toBe(1.5)
    expect('trimStart' in p.edit).toBe(false)
    expect('crop' in p.edit).toBe(false)
    expect('format' in p.edit).toBe(false)
    expect('qualityTier' in p.edit).toBe(false)
    expect(p.edit.lutId).toBe(TEST_LUT_ID)
    expect(p.edit.lutIntensity).toBe(0.75)
    expect(p.edit.curves?.red).toHaveLength(3)
    state.edit.curves.red[1].y = 1
    expect(p.edit.curves?.red[1].y).toBe(150)
    expect(JSON.parse(localStorage.getItem('ve_presets')!)).toHaveLength(1)
  })

  it('applies a preset onto the current edit', () => {
    state.edit.filter = ''
    state.edit.speed = 1
    applyPreset({ name: 'x', edit: { filter: 'warm', speed: 2 } })
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.speed).toBe(2)
  })

  it('applies the rest of a preset when its stored LUT no longer exists', async () => {
    vi.mocked(api.getLut).mockRejectedValueOnce(new api.ApiError('LUT не найден', 404))
    await applyPreset({
      name: 'stale',
      edit: { filter: 'warm', lutId: TEST_LUT_ID, lutName: 'Gone' },
    })
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.lutId).toBeNull()
  })

  it('lets the last asynchronously selected preset win', async () => {
    const resolvers = new Map<
      string,
      (asset: Awaited<ReturnType<typeof api.getLut>>) => void
    >()
    vi.mocked(api.getLut).mockImplementation(
      (id) =>
        new Promise((resolve) => {
          resolvers.set(id, resolve)
        }) as ReturnType<typeof api.getLut>,
    )

    const first = applyPreset({
      name: 'first',
      edit: { filter: 'sepia', lutId: TEST_LUT_ID },
    })
    const second = applyPreset({
      name: 'second',
      edit: { filter: 'warm', lutId: SECOND_LUT_ID },
    })
    resolvers.get(SECOND_LUT_ID)!({
      id: SECOND_LUT_ID,
      name: 'Second',
      cubeSize: 17,
      sizeBytes: 100,
    })
    await second
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.lutId).toBe(SECOND_LUT_ID)

    resolvers.get(TEST_LUT_ID)!({
      id: TEST_LUT_ID,
      name: 'First',
      cubeSize: 17,
      sizeBytes: 100,
    })
    await first
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.lutId).toBe(SECOND_LUT_ID)
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

  it('sanitizes LUT and curves loaded from storage', () => {
    localStorage.setItem(
      've_presets',
      JSON.stringify([
        {
          name: ' unsafe ',
          edit: {
            lutId: ` ${TEST_LUT_ID} `,
            lutName: ' X ',
            lutSize: 33.2,
            lutIntensity: 9,
            curves: { blue: [{ x: 128.4, y: -1 }] },
          },
        },
        { name: '', edit: {} },
      ]),
    )

    loadPresets()
    expect(presets.list).toHaveLength(1)
    expect(presets.list[0].name).toBe('unsafe')
    expect(presets.list[0].edit).toMatchObject({
      lutId: TEST_LUT_ID,
      lutName: 'X',
      lutSize: 33,
      lutIntensity: 1,
    })
    expect(presets.list[0].edit.curves?.blue).toEqual([
      { x: 0, y: 0 },
      { x: 128, y: 0 },
      { x: 255, y: 255 },
    ])
  })
})
