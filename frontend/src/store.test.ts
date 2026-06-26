import { describe, it, expect, beforeEach } from 'vitest'
import {
  state,
  defaultEdit,
  buildEditPayload,
  parseTime,
  tierToCrf,
  savePreset,
  applyPreset,
  deletePreset,
  loadPresets,
  presets,
} from './store'
import type { VideoInfo } from './types'

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
})

describe('effect presets', () => {
  beforeEach(() => {
    localStorage.clear()
    presets.list = []
    setVideo()
  })

  it('captures only reusable effect keys, not clip geometry', () => {
    Object.assign(state.edit, { filter: 'sepia', speed: 1.5, trimStart: 3, cropEnabled: true })
    state.edit.crop = { x: 1, y: 2, w: 3, h: 4 }
    savePreset('look')
    expect(presets.list).toHaveLength(1)
    const p = presets.list[0]
    expect(p.edit.filter).toBe('sepia')
    expect(p.edit.speed).toBe(1.5)
    expect('trimStart' in p.edit).toBe(false)
    expect('crop' in p.edit).toBe(false)
    expect(JSON.parse(localStorage.getItem('ve_presets')!)).toHaveLength(1)
  })

  it('applies a preset onto the current edit', () => {
    state.edit.filter = ''
    state.edit.speed = 1
    applyPreset({ name: 'x', edit: { filter: 'warm', speed: 2 } })
    expect(state.edit.filter).toBe('warm')
    expect(state.edit.speed).toBe(2)
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
