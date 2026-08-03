import { nextTick } from 'vue'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import * as api from './api'
import { createProjectDocumentFromLegacy, ensureCreatorTrackLayout, migrateProjectDocument } from './project-schema'
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
  flushProjectSave,
  timelineState,
  executeTimelineCommand,
  undoTimeline,
  addMediaToTimeline,
  doUploadFiles,
  deleteFromLibrary,
  relinkLibraryMedia,
  relinkState,
  openSavedProject,
  setProjectProxyPolicy,
  publishPlayerState,
  buildActiveMulticamFlattenPayload,
} from './store'
import { parseBrowserMulticamFlatten } from './browser-multicam-export'
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
    clientOnlyMode: false,
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
    getProjects: vi.fn(() => Promise.resolve([])),
    deleteLibraryItem: vi.fn(),
    saveProject: vi.fn(() => Promise.resolve({})),
    saveProjectDocument: vi.fn((projectId: string, expectedRevision: number, document: unknown) =>
      Promise.resolve({
        schemaVersion: 1,
        projectId,
        revision: expectedRevision + 1,
        createdAt: 1,
        updatedAt: 1,
        document,
      }),
    ),
    getProjectDocument: vi.fn(() => Promise.resolve(null)),
    getProjectByVideo: vi.fn(() => Promise.resolve(null)),
    deleteProject: vi.fn(),
    cancelJob: vi.fn(),
    getCapabilities: vi.fn(() => Promise.resolve(null)),
    getBrowserStorageStatus: vi.fn(() => null),
    resolveLibrarySource: vi.fn(),
    relinkLibrarySource: vi.fn(),
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

describe('player state bridge', () => {
  beforeEach(() => {
    state.playerTime = 0
    state.playerPlaying = false
    state.playerSeeking = false
  })

  it('publishes media element time, playback and seeking state atomically', () => {
    publishPlayerState({ time: 4.25, playing: true, seeking: true })
    expect({ time: state.playerTime, playing: state.playerPlaying, seeking: state.playerSeeking })
      .toEqual({ time: 4.25, playing: true, seeking: true })

    publishPlayerState({ time: 5, playing: false, seeking: false })
    expect({ time: state.playerTime, playing: state.playerPlaying, seeking: state.playerSeeking })
      .toEqual({ time: 5, playing: false, seeking: false })
  })

  it('ignores invalid clocks without dropping valid status changes', () => {
    publishPlayerState({ time: 3, playing: true })
    publishPlayerState({ time: Number.NaN, playing: false, seeking: true })
    expect({ time: state.playerTime, playing: state.playerPlaying, seeking: state.playerSeeking })
      .toEqual({ time: 3, playing: false, seeking: true })
  })
})

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

  it('never routes the persisted proxy preference into an export payload', () => {
    timelineState.document = createProjectDocumentFromLegacy('vid', 'Video', state.video as unknown as Record<string, unknown>, state.edit as unknown as Record<string, unknown>)
    setProjectProxyPolicy('proxy')
    const payload = buildEditPayload()
    expect(payload.videoId).toBe('vid')
    expect(JSON.stringify(payload)).not.toContain('proxy')
    expect(JSON.stringify(payload)).not.toContain('/api/proxies/')
  })

  it('emits a trim when the clip is narrowed', () => {
    state.edit.trimStart = 2
    state.edit.trimEnd = 8
    expect(buildEditPayload().trim).toEqual({ start: 2, end: 8 })
  })

  it('builds a parser-valid sliced multicam payload across a decision', () => {
    timelineState.document = createProjectDocumentFromLegacy('vid', 'Video', state.video as unknown as Record<string, unknown>, state.edit as unknown as Record<string, unknown>)
    const document = timelineState.document
    document.sequences[0]!.settings.frameRate = 25
    document.media[0]!.contentFingerprint = 'a'.repeat(64)
    document.media.push({ id: 'cam-b', kind: 'video', assetRef: 'cam-b-asset', contentFingerprint: 'b'.repeat(64), metadata: { duration: 10, width: 1920, height: 1080, fps: 30 } })
    document.multicamGroups = [{
      contract: 'multicam-v1', id: 'group', name: 'Group', timeBase: 1_000_000, durationTicks: 9_000_000,
      referenceAngleId: 'angle-a', audioAngleId: 'angle-a', sync: { method: 'marker', algorithmVersion: 'test-v1' },
      angles: [
        { id: 'angle-a', mediaId: 'vid', label: 'A', sourceOriginTick: 0, rate: { numerator: 1, denominator: 1 }, enabled: true },
        { id: 'angle-b', mediaId: 'cam-b', label: 'B', sourceOriginTick: 500_000, rate: { numerator: 1, denominator: 1 }, enabled: true },
      ],
      decisions: [{ id: 'cut-a', offsetTick: 0, angleId: 'angle-a' }, { id: 'cut-b', offsetTick: 3_000_000, angleId: 'angle-b' }],
    }]
    const clip = document.sequences[0]!.tracks[0]!.clips[0]!
    Object.assign(clip, { timelineStartTick: 2_000_000, sourceInTick: 1_000_000, sourceOutTick: 8_000_000, durationTicks: 7_000_000, multicamGroupId: 'group' })
    const parsed = parseBrowserMulticamFlatten(buildActiveMulticamFlattenPayload())
    expect(parsed.sourceStartTick).toBe(1_000_000)
    expect(parsed.timelineStartTick).toBe(2_000_000)
    expect(parsed.durationTicks).toBe(7_000_000)
    expect(parsed.intervals.map(interval => [interval.outputStartTick, interval.durationTicks])).toEqual([[0, 2_000_000], [2_000_000, 5_000_000]])
    timelineState.document = null
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
      temperature: 0.5,
      tint: -0.25,
      highlights: 0.4,
      shadows: -0.3,
      filter: 'sepia',
      vignette: true,
    })
    const p = buildEditPayload()
    expect(p.rotate).toBe(90)
    expect(p.volume).toBe(1.5)
    expect(p.brightness).toBe(0.2)
    expect(p.temperature).toBe(0.5)
    expect(p.tint).toBe(-0.25)
    expect(p.highlights).toBe(0.4)
    expect(p.shadows).toBe(-0.3)
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
    first.lift.red = 0.8
    expect(second.lift.red).toBe(0)
    expect(second.lift).not.toBe(first.lift)
    first.hslSelective.selection.centerDegrees = 180
    expect(second.hslSelective.selection.centerDegrees).toBe(0)
    expect(second.hslSelective.selection).not.toBe(first.hslSelective.selection)
  })

  it('emits one canonical colorWheels object and omits a neutral stack', () => {
    expect(buildEditPayload()).not.toHaveProperty('colorWheels')
    state.edit.lift = { master: 0.2, red: 4, green: Number.NaN, blue: -4 }
    state.edit.gamma = { master: 0, red: 0, green: 0.25, blue: 0 }
    expect(buildEditPayload().colorWheels).toEqual({
      lift: { master: 0.2, red: 1, green: 0, blue: -1 },
      gamma: { master: 0, red: 0, green: 0.25, blue: 0 },
      gain: { master: 0, red: 0, green: 0, blue: 0 },
    })
  })

  it('emits canonical selective HSL only for an active adjustment and never emits mask preview', () => {
    state.hslMaskPreview = true
    expect(buildEditPayload()).not.toHaveProperty('hslSelective')
    expect(JSON.stringify(buildEditPayload())).not.toContain('MaskPreview')

    state.edit.hslSelective = {
      selection: { centerDegrees: -1, halfWidthDegrees: 170, featherDegrees: 90 },
      adjustment: { hueDegrees: 999, saturation: Number.NaN, lightness: -9 },
    }
    expect(buildEditPayload().hslSelective).toEqual({
      selection: { centerDegrees: 359, halfWidthDegrees: 170, featherDegrees: 10 },
      adjustment: { hueDegrees: 180, saturation: 0, lightness: -1 },
    })
    state.hslMaskPreview = false
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
    expect('temperature' in payload).toBe(false)
    expect('tint' in payload).toBe(false)
    expect('highlights' in payload).toBe(false)
    expect('shadows' in payload).toBe(false)
    expect(hasMeaningfulChanges()).toBe(false)
  })

  it('clamps primary corrections and drops non-finite values from the wire', () => {
    Object.assign(state.edit, {
      temperature: 3,
      tint: -4,
      highlights: Number.NaN,
      shadows: Number.POSITIVE_INFINITY,
    })
    expect(buildEditPayload()).toMatchObject({ temperature: 1, tint: -1 })
    expect(buildEditPayload()).not.toHaveProperty('highlights')
    expect(buildEditPayload()).not.toHaveProperty('shadows')
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
      temperature: 5,
      tint: -5,
      highlights: Number.NaN,
      shadows: Number.POSITIVE_INFINITY,
      lift: { master: 5, red: -5, green: Number.NaN, blue: 0.4 },
      gamma: null,
      hslSelective: {
        selection: { centerDegrees: 721, halfWidthDegrees: 170, featherDegrees: 50 },
        adjustment: { hueDegrees: -999, saturation: 3, lightness: Number.NaN },
      },
    })
    expect(edit.lutId).toBe(TEST_LUT_ID)
    expect(edit.lutName).toBe('Look')
    expect(edit.lutSize).toBe(65)
    expect(edit.lutIntensity).toBe(1)
    expect(edit).toMatchObject({ temperature: 1, tint: -1, highlights: 0, shadows: 0 })
    expect(edit.lift).toEqual({ master: 1, red: -1, green: 0, blue: 0.4 })
    expect(edit.gamma).toEqual({ master: 0, red: 0, green: 0, blue: 0 })
    expect(edit.hslSelective).toEqual({
      selection: { centerDegrees: 1, halfWidthDegrees: 170, featherDegrees: 10 },
      adjustment: { hueDegrees: -180, saturation: 1, lightness: 0 },
    })
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
      temperature: 0.8,
      tint: -0.7,
      highlights: 0.6,
      shadows: -0.5,
      filter: 'warm',
      lutId: TEST_LUT_ID,
      lutName: 'Look',
      lutSize: 17,
      lutIntensity: 0.5,
      vignette: true,
      denoise: true,
      sharpen: 2,
      grain: 10,
      lift: { master: 0.2, red: -0.3, green: 0.4, blue: 0 },
      gamma: { master: -0.2, red: 0, green: 0, blue: 0 },
      gain: { master: 0, red: 0, green: 0.5, blue: 0 },
      hslSelective: {
        selection: { centerDegrees: 220, halfWidthDegrees: 20, featherDegrees: 8 },
        adjustment: { hueDegrees: 30, saturation: 0.2, lightness: -0.3 },
      },
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
      temperature: 0,
      tint: 0,
      highlights: 0,
      shadows: 0,
      lift: { master: 0, red: 0, green: 0, blue: 0 },
      gamma: { master: 0, red: 0, green: 0, blue: 0 },
      gain: { master: 0, red: 0, green: 0, blue: 0 },
      hslSelective: {
        selection: { centerDegrees: 0, halfWidthDegrees: 30, featherDegrees: 15 },
        adjustment: { hueDegrees: 0, saturation: 0, lightness: 0 },
      },
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

  it('fails closed when primary corrections are unsupported', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [],
      hardware: [],
    }
    state.edit.temperature = 0.25
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')
    state.capabilities.filters.push({
      id: 'primary-corrections', label: 'Primary corrections', available: false, reason: 'нет geq',
    })
    expect(selectedExportUnavailableReason()).toBe('нет geq')
    state.capabilities.filters[0]!.available = true
    expect(selectedExportUnavailableReason()).toBeNull()
  })

  it('fails closed when Lift/Gamma/Gain are unsupported', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [],
      hardware: [],
    }
    state.edit.gain.red = 0.25
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')
    state.capabilities.filters.push({
      id: 'color-wheels', label: 'Lift/Gamma/Gain', available: true,
    })
    expect(selectedExportUnavailableReason()).toBeNull()
  })

  it('fails closed when Selective HSL is unsupported', () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'fixture',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [],
      hardware: [],
    }
    state.edit.hslSelective.adjustment.hueDegrees = 10
    expect(selectedExportUnavailableReason()).toContain('обновлённый сервер')
    state.capabilities.filters.push({
      id: 'hsl-selective-v1', label: 'Selective HSL', available: true,
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

  it('keeps Selective HSL mask preview outside edit history', async () => {
    state.hslMaskPreview = false
    state.hslMaskPreview = true
    await nextTick()
    vi.advanceTimersByTime(500)
    expect(history.past).toHaveLength(0)
    state.hslMaskPreview = false
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

  it('opens an offline project as placeholders without racing a late source resolve', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'offline-project', kind: 'source', filename: 'missing.mp4', url: '',
      availability: 'offline', duration: 5, width: 640, height: 360, createdAt: 1,
    })
    await Promise.resolve()
    expect(state.video).toMatchObject({ id: 'offline-project', url: '', availability: 'offline' })
    expect(api.resolveLibrarySource).not.toHaveBeenCalled()
    expect(timelineState.document?.primaryMediaId).toBe('offline-project')
  })

  it('does not let a late ready-source materialization replace a newer project', async () => {
    let resolveOld!: (value: VideoInfo) => void
    vi.mocked(api.resolveLibrarySource).mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve }))
    vi.mocked(api.getProjectByVideo).mockResolvedValue(null)
    openFromLibrary({ id: 'slow-a', kind: 'source', filename: 'a.mp4', url: '', availability: 'ready', createdAt: 1 })
    openFromLibrary({ id: 'fast-b', kind: 'source', filename: 'b.mp4', url: '/b.mp4', availability: 'ready', createdAt: 2 })
    resolveOld({ id: 'slow-a', filename: 'a.mp4', url: 'blob:a', duration: 1, width: 1, height: 1 })
    await Promise.resolve()
    expect(state.video?.id).toBe('fast-b')
  })

  it('fails closed when selecting a project that expects another source fingerprint', () => {
    const video: VideoInfo = { id: 'shared', filename: 'shared.mp4', url: 'blob:a', duration: 1, width: 1, height: 1, fingerprint: 'fingerprint-a' }
    const document = createProjectDocumentFromLegacy('shared', 'Project B', video as unknown as Record<string, unknown>, {})
    document.media[0]!.assetRef = 'shared'
    document.media[0]!.contentFingerprint = 'fingerprint-b'
    state.library = [{ id: 'shared', kind: 'source', filename: 'shared.mp4', url: 'blob:a', availability: 'ready', fingerprint: 'fingerprint-a', createdAt: 1 }]
    vi.mocked(api.resolveLibrarySource).mockRejectedValueOnce(new Error('fingerprint mismatch'))
    openSavedProject({ id: 'project-b', name: 'Project B', videoId: 'shared', video, edit: {}, document, revision: 1, createdAt: 1, updatedAt: 1 })
    return vi.waitFor(() => expect(state.video).toMatchObject({ id: 'shared', url: '', availability: 'offline', fingerprint: 'fingerprint-b' }))
  })

  it('does not let a stale relink failure revert a newer successful relink', async () => {
    const entry = {
      id: 'race-source', kind: 'source' as const, filename: 'source.mp4', url: '',
      availability: 'offline' as const, createdAt: 1,
    }
    let rejectFirst!: (reason: Error) => void
    let resolveSecond!: (value: VideoInfo) => void
    vi.mocked(api.relinkLibrarySource)
      .mockReturnValueOnce(new Promise((_resolve, reject) => { rejectFirst = reject }))
      .mockReturnValueOnce(new Promise((resolve) => { resolveSecond = resolve }))
    const first = relinkLibraryMedia(entry, new File(['bad'], 'bad.mp4'))
    const second = relinkLibraryMedia(entry, new File(['good'], 'good.mp4'))
    rejectFirst(new Error('wrong fingerprint'))
    await expect(first).resolves.toBe(false)
    resolveSecond({ id: entry.id, filename: entry.filename, url: 'blob:ready', duration: 1, width: 1, height: 1 })
    await expect(second).resolves.toBe(true)
    expect(entry).toMatchObject({ availability: 'ready', url: 'blob:ready' })
    expect(relinkState.busy[entry.id]).toBe(false)
  })

  it('re-audits durable identity when a newer wrong relink follows an exact one', async () => {
    const entry = {
      id: 'inverse-race', kind: 'source' as const, filename: 'source.mp4', url: '',
      availability: 'offline' as const, createdAt: 1,
    }
    const ready: VideoInfo = { id: entry.id, filename: entry.filename, url: 'blob:ready', duration: 1, width: 1, height: 1 }
    vi.mocked(api.relinkLibrarySource)
      .mockResolvedValueOnce(ready)
      .mockRejectedValueOnce(Object.assign(new Error('wrong fingerprint'), { reason: 'fingerprint' }))
    vi.mocked(api.resolveLibrarySource).mockResolvedValueOnce(ready)
    const exact = relinkLibraryMedia(entry, new File(['exact'], 'renamed.mp4'))
    const wrong = relinkLibraryMedia(entry, new File(['wrong'], 'wrong.mp4'))
    await expect(exact).resolves.toBe(false)
    await expect(wrong).resolves.toBe(false)
    expect(entry).toMatchObject({ availability: 'ready', url: 'blob:ready' })
  })

  it('blocks media insertion until a pending project restore finishes', async () => {
    let resolveProject!: (value: Awaited<ReturnType<typeof api.getProjectByVideo>>) => void
    vi.mocked(api.getProjectByVideo).mockReturnValueOnce(new Promise((resolve) => {
      resolveProject = resolve
    }))
    openFromLibrary({
      id: 'restore-race', kind: 'source', filename: 'primary.mp4', url: '/primary',
      duration: 5, width: 1280, height: 720, mediaKind: 'video', createdAt: 1,
    })
    expect(addMediaToTimeline({
      id: 'added-during-restore', kind: 'source', filename: 'added.mp4', url: '/added',
      duration: 2, width: 640, height: 360, mediaKind: 'video', createdAt: 2,
    })).toBe(false)
    expect(timelineState.error).toContain('Дождитесь')
    resolveProject({
      id: 'persisted-project',
      name: 'Persisted',
      videoId: 'restore-race',
      video: {
        id: 'restore-race', filename: 'primary.mp4', url: '/primary',
        duration: 5, width: 1280, height: 720,
      },
      edit: { filter: 'sepia' },
      createdAt: 1,
      updatedAt: 2,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()

    expect(timelineState.document?.media.some((media) => media.id === 'added-during-restore'))
      .toBe(false)
    expect(state.edit.filter).toBe('sepia')
  })

  it('opens an audio-only source as an audio project', () => {
    openFromLibrary({
      id: 'audio-primary',
      kind: 'source',
      filename: 'voice.wav',
      url: '/files/sources/voice.wav',
      duration: 3,
      width: 0,
      height: 0,
      mediaKind: 'audio',
      acodec: 'pcm_s16le',
      createdAt: 1,
    })
    expect(timelineState.document?.media[0]).toMatchObject({
      id: 'audio-primary', kind: 'audio',
    })
    const sequence = timelineState.document!.sequences[0]!
    expect(sequence.tracks.find((track) => track.kind === 'video')!.clips).toHaveLength(0)
    expect(sequence.tracks.find((track) => track.kind === 'audio')!.clips[0])
      .toMatchObject({ mediaId: 'audio-primary' })
  })

  it('autosaves the first edit when no persisted project exists', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'new-project',
      kind: 'source',
      filename: 'new.mp4',
      url: '/files/sources/new.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.saveProjectDocument).mockClear()

    state.edit.filter = 'sepia'
    await nextTick()
    await vi.advanceTimersByTimeAsync(1000)

    expect(api.saveProjectDocument).toHaveBeenCalledTimes(1)
  })

  it('persists a structural timeline edit in the canonical document', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'timeline-save',
      kind: 'source',
      filename: 'timeline.mp4',
      url: '/files/sources/timeline.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.saveProjectDocument).mockClear()
    const document = timelineState.document!
    const clip = document.sequences[0]!.tracks[0]!.clips[0]!
    expect(
      executeTimelineCommand({
        kind: 'trim_clip',
        sequenceId: document.activeSequenceId,
        clipId: clip.id,
        sourceInTick: 1_000_000,
        sourceOutTick: 9_000_000,
        timelineStartTick: 1_000_000,
      }),
    ).toBe(true)
    await flushProjectSave()

    const saved = vi.mocked(api.saveProjectDocument).mock.calls.at(-1)?.[2]
    const savedClip = saved?.sequences[0]?.tracks[0]?.clips[0]
    expect(savedClip).toMatchObject({
      sourceInTick: 1_000_000,
      sourceOutTick: 9_000_000,
      timelineStartTick: 1_000_000,
      durationTicks: 8_000_000,
    })
  })

  it('does not let a stale autosave response overwrite a newer structural edit', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'timeline-race',
      kind: 'source',
      filename: 'timeline-race.mp4',
      url: '/files/sources/timeline-race.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.saveProjectDocument).mockClear()

    let resolveFirst!: (value: Awaited<ReturnType<typeof api.saveProjectDocument>>) => void
    vi.mocked(api.saveProjectDocument)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve
          }),
      )
      .mockImplementation((projectId, expectedRevision, document) =>
        Promise.resolve({
          schemaVersion: 1,
          projectId,
          revision: expectedRevision + 1,
          createdAt: 1,
          updatedAt: 1,
          document,
        }),
      )

    const document = timelineState.document!
    const clip = document.sequences[0]!.tracks[0]!.clips[0]!
    expect(
      executeTimelineCommand({
        kind: 'trim_clip',
        sequenceId: document.activeSequenceId,
        clipId: clip.id,
        sourceInTick: 1_000_000,
        sourceOutTick: 9_000_000,
        timelineStartTick: 1_000_000,
      }),
    ).toBe(true)
    const saving = flushProjectSave()
    await Promise.resolve()
    const firstCall = vi.mocked(api.saveProjectDocument).mock.calls[0]!

    expect(
      executeTimelineCommand({
        kind: 'trim_clip',
        sequenceId: document.activeSequenceId,
        clipId: clip.id,
        sourceInTick: 2_000_000,
        sourceOutTick: 8_000_000,
        timelineStartTick: 2_000_000,
      }),
    ).toBe(true)
    resolveFirst({
      schemaVersion: 1,
      projectId: firstCall[0],
      revision: firstCall[1] + 1,
      createdAt: 1,
      updatedAt: 1,
      document: firstCall[2],
    })
    await saving

    expect(api.saveProjectDocument).toHaveBeenCalledTimes(2)
    const newest = vi.mocked(api.saveProjectDocument).mock.calls[1]![2]
    expect(newest.sequences[0]!.tracks[0]!.clips[0]).toMatchObject({
      sourceInTick: 2_000_000,
      sourceOutTick: 8_000_000,
      timelineStartTick: 2_000_000,
    })
    expect(timelineState.document?.sequences[0]!.tracks[0]!.clips[0]).toMatchObject({
      sourceInTick: 2_000_000,
      sourceOutTick: 8_000_000,
      timelineStartTick: 2_000_000,
    })
  })

  it('saves an empty timeline after deleting the last clip and saves its undo', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'timeline-empty',
      kind: 'source',
      filename: 'timeline-empty.mp4',
      url: '/files/sources/timeline-empty.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.saveProjectDocument).mockClear()
    const document = timelineState.document!
    const track = document.sequences[0]!.tracks[0]!
    expect(executeTimelineCommand({
      kind: 'remove_clip',
      sequenceId: document.activeSequenceId,
      trackId: track.id,
      clipId: track.clips[0]!.id,
    })).toBe(true)
    await flushProjectSave()
    expect(vi.mocked(api.saveProjectDocument).mock.calls.at(-1)![2]
      .sequences[0]!.tracks[0]!.clips).toHaveLength(0)

    expect(undoTimeline()).toBe(true)
    await flushProjectSave()
    expect(vi.mocked(api.saveProjectDocument).mock.calls.at(-1)![2]
      .sequences[0]!.tracks[0]!.clips).toHaveLength(1)
  })

  it('does not delete a primary asset still referenced by an empty timeline', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    const entry = {
      id: 'protected-primary', kind: 'source' as const, filename: 'protected.mp4',
      url: '/protected', duration: 5, width: 1280, height: 720,
      mediaKind: 'video' as const, createdAt: 1,
    }
    state.library = [entry]
    openFromLibrary(entry)
    await Promise.resolve()
    await nextTick()
    const document = timelineState.document!
    const track = document.sequences[0]!.tracks[0]!
    expect(executeTimelineCommand({
      kind: 'remove_clip', sequenceId: document.activeSequenceId,
      trackId: track.id, clipId: track.clips[0]!.id,
    })).toBe(true)

    await deleteFromLibrary(entry.id)
    expect(api.deleteLibraryItem).not.toHaveBeenCalled()
    expect(state.library).toContainEqual(entry)
  })

  it('adds twenty public media entries without replacing the active project and persists them', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'primary-media',
      kind: 'source',
      filename: 'primary.mp4',
      url: '/files/sources/primary.mp4',
      duration: 10,
      width: 1920,
      height: 1080,
      fps: 30,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.saveProjectDocument).mockClear()

    const primaryVideo = state.video
    for (let index = 0; index < 20; index++) {
      expect(addMediaToTimeline({
        id: `secondary-${index}`,
        kind: 'source',
        filename: `secondary-${index}.mp4`,
        url: `/files/sources/secondary-${index}.mp4`,
        duration: 1 + index / 10,
        width: index === 19 ? 0 : index % 2 ? 1280 : 3840,
        height: index === 19 ? 0 : index % 2 ? 720 : 2160,
        fps: [23.976, 25, 29.97, 59.94][index % 4],
        vcodec: index === 19 ? null : 'h264',
        acodec: 'aac',
        availability: index === 0 ? 'ready' : undefined,
        createdAt: index + 2,
      })).toBe(true)
    }
    expect(state.video).toBe(primaryVideo)
    expect(timelineState.document?.primaryMediaId).toBe('primary-media')
    expect(timelineState.document?.media).toHaveLength(21)
    expect(timelineState.document?.sequences[0]!.tracks.flatMap((track) => track.clips)).toHaveLength(21)

    const duplicate = {
      id: 'secondary-0',
      kind: 'source' as const,
      filename: 'secondary-0.mp4',
      url: '/files/sources/secondary-0.mp4',
      duration: 1,
      width: 3840,
      height: 2160,
      fps: 23.976,
      createdAt: 2,
    }
    expect(addMediaToTimeline(duplicate)).toBe(true)
    expect(timelineState.document?.media).toHaveLength(21)
    expect(timelineState.document?.sequences[0]!.tracks.flatMap((track) => track.clips)).toHaveLength(22)

    await flushProjectSave()
    const saved = vi.mocked(api.saveProjectDocument).mock.calls.at(-1)![2]
    expect(saved.primaryMediaId).toBe('primary-media')
    expect(saved.media).toHaveLength(21)
    expect(saved.media.find((media) => media.id === 'secondary-0')?.metadata)
      .not.toHaveProperty('availability')
    expect(saved.sequences[0]!.tracks.flatMap((track) => track.clips)).toHaveLength(22)
    expect(selectedExportUnavailableReason()).toContain('render graph')
  })

  it('rejects offline media without mutating the active project or history', () => {
    timelineState.document = ensureCreatorTrackLayout(migrateProjectDocument({
      videoId: 'primary',
      video: { id: 'primary', filename: 'primary.mp4', duration: 5, width: 1280, height: 720 },
      edit: {},
    }))
    const before = JSON.stringify(timelineState.document)
    const canUndoBefore = timelineState.canUndo

    expect(addMediaToTimeline({
      id: 'offline', kind: 'source', filename: 'offline.mp4', url: '',
      duration: 3, width: 1920, height: 1080, mediaKind: 'video',
      availability: 'offline', createdAt: 2,
    })).toBe(false)

    expect(timelineState.error).toContain('найдите исходный файл')
    expect(JSON.stringify(timelineState.document)).toBe(before)
    expect(timelineState.canUndo).toBe(canUndoBefore)
  })

  it('bulk upload adds every successful file to the same active project', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'bulk-primary',
      kind: 'source',
      filename: 'primary.mp4',
      url: '/files/sources/primary.mp4',
      duration: 5,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    vi.mocked(api.uploadFile)
      .mockResolvedValueOnce({
        id: 'bulk-video', url: '/video', filename: 'video.mp4', duration: 2,
        width: 1920, height: 1080, fps: 25,
      })
      .mockResolvedValueOnce({
        id: 'bulk-audio', url: '/audio', filename: 'audio.wav', duration: 3,
        width: 0, height: 0, acodec: 'pcm_s16le',
      })

    await doUploadFiles([
      new File(['video'], 'video.mp4', { type: 'video/mp4' }),
      new File(['audio'], 'audio.wav', { type: 'audio/wav' }),
    ])

    expect(state.video?.id).toBe('bulk-primary')
    expect(timelineState.document?.media.map((media) => media.id)).toEqual([
      'bulk-primary', 'bulk-video', 'bulk-audio',
    ])
    const tracks = timelineState.document!.sequences[0]!.tracks
    expect(tracks.find((track) => track.kind === 'video')!.clips).toHaveLength(2)
    expect(tracks.find((track) => track.kind === 'audio')!.clips).toHaveLength(1)
  })

  it('blocks legacy export when only a secondary clip remains', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValueOnce(null)
    openFromLibrary({
      id: 'topology-primary', kind: 'source', filename: 'primary.mp4', url: '/primary',
      duration: 5, width: 1280, height: 720, mediaKind: 'video', createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()
    expect(addMediaToTimeline({
      id: 'topology-secondary', kind: 'source', filename: 'secondary.mp4', url: '/secondary',
      duration: 2, width: 640, height: 360, mediaKind: 'video', createdAt: 2,
    })).toBe(true)
    const document = timelineState.document!
    const primaryTrack = document.sequences[0]!.tracks.find((track) =>
      track.clips.some((clip) => clip.mediaId === 'topology-primary'),
    )!
    expect(executeTimelineCommand({
      kind: 'remove_clip', sequenceId: document.activeSequenceId,
      trackId: primaryTrack.id,
      clipId: primaryTrack.clips.find((clip) => clip.mediaId === 'topology-primary')!.id,
    })).toBe(true)

    expect(selectedExportUnavailableReason()).toContain('topology')
  })

  it('does not attach a completed upload after the user switches projects', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValue(null)
    openFromLibrary({
      id: 'project-a', kind: 'source', filename: 'a.mp4', url: '/a',
      duration: 5, width: 1280, height: 720, mediaKind: 'video', createdAt: 1,
    })
    await Promise.resolve()
    await nextTick()
    let resolveUpload!: (value: VideoInfo) => void
    vi.mocked(api.uploadFile).mockReturnValueOnce(new Promise((resolve) => {
      resolveUpload = resolve
    }))
    const uploading = doUploadFiles([new File(['b'], 'b.mp4', { type: 'video/mp4' })])

    openFromLibrary({
      id: 'project-a', kind: 'source', filename: 'reopened-a.mp4', url: '/c',
      duration: 5, width: 1920, height: 1080, mediaKind: 'video', createdAt: 2,
    })
    resolveUpload({
      id: 'late-b', url: '/b', filename: 'b.mp4', duration: 2,
      width: 640, height: 360, mediaKind: 'video',
    })
    await uploading

    expect(state.video?.id).toBe('project-a')
    expect(state.video?.filename).toBe('reopened-a.mp4')
    expect(timelineState.document?.primaryMediaId).toBe('project-a')
    expect(timelineState.document?.media.some((media) => media.id === 'late-b')).toBe(false)
    expect(state.importError).toContain('проект изменился во время загрузки')
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
    await Promise.resolve()
    await nextTick()

    expect(state.edit.lutId).toBeNull()
    expect(state.edit.curves.blue).toEqual(curves.blue)
  })

  it('keeps programmatic edits while restore is gated', async () => {
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
    expect(api.saveProjectDocument).not.toHaveBeenCalled()
  })

  it('coalesces an edit made while autosave is in flight into the next revision', async () => {
    openFromLibrary({
      id: 'single-flight',
      kind: 'source',
      filename: 'single-flight.mp4',
      url: '/files/sources/single-flight.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      createdAt: 1,
    })
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
    await flushProjectSave()
    await Promise.resolve()
    await Promise.resolve()
    vi.mocked(api.saveProjectDocument).mockClear()
    let resolveFirst: (value: Awaited<ReturnType<typeof api.saveProjectDocument>>) => void = () => {}
    vi.mocked(api.saveProjectDocument)
      .mockImplementationOnce(() =>
        new Promise((resolve) => {
          resolveFirst = resolve
        }) as ReturnType<typeof api.saveProjectDocument>,
      )
      .mockImplementationOnce((projectId, expectedRevision, document) =>
        Promise.resolve({
          schemaVersion: 1,
          projectId,
          revision: expectedRevision + 1,
          createdAt: 1,
          updatedAt: 2,
          document,
        }),
      )

    state.edit.filter = 'sepia'
    const firstSave = flushProjectSave()
    await Promise.resolve()
    expect(api.saveProjectDocument, timelineState.error).toHaveBeenCalledTimes(1)
    const [projectId, expectedRevision, firstDocument] = vi.mocked(api.saveProjectDocument).mock.calls[0]!

    state.edit.filter = 'warm'
    const secondSave = flushProjectSave()
    expect(api.saveProjectDocument).toHaveBeenCalledTimes(1)

    resolveFirst({
      schemaVersion: 1,
      projectId,
      revision: expectedRevision + 1,
      createdAt: 1,
      updatedAt: 1,
      document: firstDocument,
    })
    await Promise.resolve()
    await Promise.resolve()
    await Promise.all([firstSave, secondSave])

    expect(api.saveProjectDocument).toHaveBeenCalledTimes(2)
    expect(vi.mocked(api.saveProjectDocument).mock.calls[1]?.[1]).toBe(expectedRevision + 1)
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
    state.hslMaskPreview = true
    Object.assign(state.edit, {
      filter: 'sepia',
      speed: 1.5,
      temperature: 0.4,
      tint: -0.3,
      highlights: 0.2,
      shadows: -0.1,
      lift: { master: 0.2, red: 0.1, green: 0, blue: -0.1 },
      gamma: { master: -0.2, red: 0, green: 0, blue: 0 },
      gain: { master: 0.3, red: 0, green: 0.1, blue: 0 },
      hslSelective: {
        selection: { centerDegrees: 45, halfWidthDegrees: 20, featherDegrees: 5 },
        adjustment: { hueDegrees: -15, saturation: 0.2, lightness: -0.1 },
      },
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
    expect(p.edit).toMatchObject({ temperature: 0.4, tint: -0.3, highlights: 0.2, shadows: -0.1 })
    expect(p.edit.lift).toEqual({ master: 0.2, red: 0.1, green: 0, blue: -0.1 })
    expect(p.edit.gamma?.master).toBe(-0.2)
    expect(p.edit.gain?.green).toBe(0.1)
    expect(p.edit.hslSelective).toEqual({
      selection: { centerDegrees: 45, halfWidthDegrees: 20, featherDegrees: 5 },
      adjustment: { hueDegrees: -15, saturation: 0.2, lightness: -0.1 },
    })
    expect(JSON.stringify(p.edit)).not.toContain('MaskPreview')
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
    state.hslMaskPreview = false
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
