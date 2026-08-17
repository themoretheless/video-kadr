import { describe, expect, it } from 'vitest'
import { buildEditPayload, defaultEdit } from '../../domain/edit'
import type { EditState, VideoInfo } from '../../types'
import {
  ASPECT_PRESETS,
  cropForAspect,
  findAspect,
  findPlatform,
  framingForAspect,
  isAspectActive,
  platformPatch,
} from './exportPresets'

const VIDEO: VideoInfo = {
  id: 'vid',
  url: '/files/sources/vid.mp4',
  filename: 'vid.mp4',
  duration: 10,
  width: 1920,
  height: 1080,
}

function editFor(video: VideoInfo): EditState {
  const edit = defaultEdit()
  edit.trimEnd = video.duration
  edit.crop = { x: 0, y: 0, w: video.width, h: video.height }
  edit.scale = { w: video.width, h: -2 }
  return edit
}

function aspect(id: string) {
  const preset = findAspect(id)
  if (!preset) throw new Error(`unknown aspect ${id}`)
  return preset
}

describe('aspect presets', () => {
  it('centres an even-sided crop inside the source frame', () => {
    expect(cropForAspect(VIDEO, { w: 9, h: 16 })).toEqual({ x: 657, y: 0, w: 606, h: 1080 })
    expect(cropForAspect(VIDEO, { w: 1, h: 1 })).toEqual({ x: 420, y: 0, w: 1080, h: 1080 })
    expect(cropForAspect(VIDEO, { w: 16, h: 9 })).toEqual({ x: 0, y: 0, w: 1920, h: 1080 })
  })

  it('never crops and pads at the same time', () => {
    const cropped = framingForAspect(VIDEO, aspect('4:5'), 'crop')
    expect(cropped.cropEnabled).toBe(true)
    expect(cropped.pad).toBe('')

    const padded = framingForAspect(VIDEO, aspect('4:5'), 'pad')
    expect(padded.cropEnabled).toBe(false)
    expect(padded.pad).toBe('4:5')
  })

  it('sends 2.39:1 as an integer ratio the backend can parse', () => {
    const cinemascope = aspect('2.39:1')
    expect(cinemascope.ratio.w).toBeLessThanOrEqual(100)
    expect(cinemascope.ratio.h).toBeLessThanOrEqual(100)
    expect(cinemascope.ratio.w / cinemascope.ratio.h).toBeCloseTo(2.39, 2)
    expect(framingForAspect(VIDEO, cinemascope, 'pad').pad).toBe('98:41')
  })

  it('reports the active preset for the current framing only', () => {
    const edit = editFor(VIDEO)
    const framing = framingForAspect(VIDEO, aspect('1:1'), 'crop')
    edit.cropEnabled = framing.cropEnabled
    edit.crop = framing.crop
    expect(isAspectActive(edit, aspect('1:1'), 'crop')).toBe(true)
    expect(isAspectActive(edit, aspect('9:16'), 'crop')).toBe(false)
    expect(isAspectActive(edit, aspect('1:1'), 'pad')).toBe(false)
  })

  it('lists every ratio the panel offers', () => {
    expect(ASPECT_PRESETS.map((preset) => preset.id)).toEqual([
      '9:16',
      '1:1',
      '4:5',
      '16:9',
      '2.39:1',
    ])
  })
})

describe('platform presets', () => {
  function patchFor(id: string) {
    const preset = findPlatform(id)
    if (!preset) throw new Error(`unknown platform ${id}`)
    return platformPatch(VIDEO, preset)
  }

  it('builds a vertical 1080x1920 crop for the short-video platforms', () => {
    for (const id of ['reels', 'shorts', 'tiktok']) {
      const patch = patchFor(id)
      expect(patch).toMatchObject({
        cropEnabled: true,
        pad: '',
        scaleEnabled: true,
        scale: { w: 1080, h: -2 },
        fps: 30,
        format: 'mp4',
        codec: 'h264',
      })
      expect(patch.crop.w / patch.crop.h).toBeCloseTo(9 / 16, 2)
    }
  })

  it('letterboxes for YouTube instead of throwing pixels away', () => {
    const patch = patchFor('youtube')
    expect(patch.cropEnabled).toBe(false)
    expect(patch.pad).toBe('16:9')
    expect(patch.scale).toEqual({ w: 1920, h: -2 })
    expect(patch.fps).toBeNull()
  })

  it('reaches the wire as crop, scale and pad on the edit payload', () => {
    const edit = editFor(VIDEO)
    Object.assign(edit, patchFor('reels'))
    const payload = buildEditPayload(edit, VIDEO)

    expect(payload.crop).toEqual(edit.crop)
    expect(payload.scale).toEqual({ w: 1080, h: -2 })
    expect(payload.fps).toBe(30)
    expect(payload.pad).toBeUndefined()

    const padded = editFor(VIDEO)
    Object.assign(padded, patchFor('youtube'))
    const paddedPayload = buildEditPayload(padded, VIDEO)
    expect(paddedPayload.pad).toBe('16:9')
    expect(paddedPayload.crop).toBeUndefined()
  })
})
