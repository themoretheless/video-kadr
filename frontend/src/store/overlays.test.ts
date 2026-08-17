import { beforeEach, describe, expect, it } from 'vitest'
import { contentBox } from '../domain/contentBox'
import type { AssetEntry } from '../types'
import { assetsState } from './assets'
import {
  addOverlay,
  addTitle,
  applyEyedropper,
  applyOverlaysSnapshot,
  attachSubtitles,
  clearSubtitles,
  createOverlay,
  cueIndexAt,
  dragOverlay,
  dragTitle,
  isSelected,
  markCuesDirty,
  moveOverlay,
  moveTitle,
  overlayAspect,
  overlaysPayload,
  overlaysState,
  overlaysUi,
  overlayVisibleAt,
  parseSubtitleCues,
  removeCue,
  removeOverlay,
  removeTitle,
  resetOverlays,
  selectItem,
  serializeSubtitleCues,
  setSubtitleCues,
  startEyedropper,
  titleVisibleAt,
} from './overlays'

const IMAGE: AssetEntry = {
  id: 'ast_image01234567890',
  kind: 'image',
  filename: 'logo.png',
  mime: 'image/png',
  sizeBytes: 1024,
  sha256: '',
  width: 400,
  height: 200,
  duration: null,
}

const CLIP: AssetEntry = {
  ...IMAGE,
  id: 'ast_clip012345678901',
  kind: 'video',
  filename: 'pip.mp4',
  mime: 'video/mp4',
  width: 1920,
  height: 1080,
  duration: 8,
}

const SUBTITLE_ID = 'ast_subs0123456789ab'

beforeEach(() => {
  resetOverlays()
  assetsState.list = [IMAGE, CLIP]
})

describe('title list editing', () => {
  it('adds, selects and serializes a title', () => {
    expect(addTitle()).toBe(0)
    expect(isSelected('title', 0)).toBe(true)
    overlaysState.titles[0].text = 'Привет'

    const payload = overlaysPayload()
    expect(payload.titles).toEqual([
      { text: 'Привет', fontSize: 48, color: '#FFFFFF', x: 0.5, y: 0.85, align: 'center' },
    ])
  })

  it('drops a title whose text was emptied instead of sending it', () => {
    addTitle()
    overlaysState.titles[0].text = '   '
    expect(overlaysPayload()).toEqual({})
  })

  it('reorders titles and keeps the selection on the moved entry', () => {
    addTitle()
    overlaysState.titles[0].text = 'Первый'
    addTitle()
    overlaysState.titles[1].text = 'Второй'

    moveTitle(1, -1)
    expect(overlaysState.titles.map((title) => title.text)).toEqual(['Второй', 'Первый'])
    expect(isSelected('title', 0)).toBe(true)
  })

  it('ignores a move that would leave the list', () => {
    addTitle()
    moveTitle(0, -1)
    moveTitle(0, 5)
    expect(overlaysState.titles).toHaveLength(1)
  })

  it('clears a selection the removal invalidated', () => {
    addTitle()
    addTitle()
    selectItem('title', 1)
    removeTitle(1)
    expect(overlaysUi.selection).toBeNull()
    // An out-of-range index is a no-op, not a crash.
    removeTitle(7)
    expect(overlaysState.titles).toHaveLength(1)
  })
})

describe('overlay list editing', () => {
  it('builds an image overlay without an audio branch', () => {
    expect(addOverlay(IMAGE)).toBe(0)
    const overlay = overlaysState.overlays[0]
    expect(overlay.kind).toBe('image')
    expect(overlay.audio).toBeNull()
    expect(overlay.height).toBeNull()

    expect(overlaysPayload().overlays).toEqual([
      { assetId: IMAGE.id, kind: 'image', x: 0.05, y: 0.05, width: 0.25, height: null },
    ])
  })

  it('gives a video overlay a disabled audio branch that stays off the wire', () => {
    addOverlay(CLIP)
    expect(overlaysState.overlays[0].audio).toEqual({ enabled: false, volume: 1 })
    const payload = overlaysPayload().overlays as Record<string, unknown>[]
    expect(payload[0].audio).toBeUndefined()

    overlaysState.overlays[0].audio = { enabled: true, volume: 0.5 }
    const enabled = overlaysPayload().overlays as Record<string, unknown>[]
    expect(enabled[0].audio).toEqual({ enabled: true, volume: 0.5 })
  })

  it('refuses an asset kind that cannot be composited', () => {
    expect(createOverlay({ ...IMAGE, kind: 'font' })).toBeNull()
    expect(addOverlay({ ...IMAGE, kind: 'subtitle' })).toBe(-1)
    expect(overlaysState.overlays).toHaveLength(0)
  })

  it('reads the aspect ratio from the asset library', () => {
    addOverlay(IMAGE)
    expect(overlayAspect(overlaysState.overlays[0])).toBeCloseTo(2)

    assetsState.list = []
    expect(overlayAspect(overlaysState.overlays[0])).toBeNull()
  })

  it('reorders overlays, which is their stacking order', () => {
    addOverlay(IMAGE)
    addOverlay(CLIP)
    moveOverlay(0, 1)
    expect(overlaysState.overlays.map((overlay) => overlay.assetId)).toEqual([CLIP.id, IMAGE.id])
    expect(isSelected('overlay', 1)).toBe(true)
    removeOverlay(0)
    expect(overlaysState.overlays).toHaveLength(1)
  })

  it('chroma key and opacity survive the round trip to the wire', () => {
    addOverlay(CLIP)
    const overlay = overlaysState.overlays[0]
    overlay.chromaKey = { color: '#00FF00', similarity: 0.2, blend: 0.1 }
    overlay.opacity = 0.6
    overlay.rotation = 15

    const payload = overlaysPayload().overlays as Record<string, unknown>[]
    expect(payload[0].chromaKey).toEqual({ color: '#00FF00', similarity: 0.2, blend: 0.1 })
    expect(payload[0].opacity).toBe(0.6)
    expect(payload[0].rotation).toBe(15)
  })
})

describe('visibility gating', () => {
  it('follows the start/end window on the output timeline', () => {
    addTitle()
    const title = overlaysState.titles[0]
    title.start = 2
    title.end = 5
    expect(titleVisibleAt(title, 1.9)).toBe(false)
    expect(titleVisibleAt(title, 3)).toBe(true)
    expect(titleVisibleAt(title, 5.1)).toBe(false)

    title.end = null
    expect(titleVisibleAt(title, 500)).toBe(true)
  })

  it('treats an open-ended overlay as visible from its start', () => {
    addOverlay(IMAGE)
    const overlay = overlaysState.overlays[0]
    overlay.start = 4
    expect(overlayVisibleAt(overlay, 3.5)).toBe(false)
    expect(overlayVisibleAt(overlay, 4)).toBe(true)
  })
})

describe('live preview dragging', () => {
  // A 16:9 source in a square element: the content box is 800x450 with 175px
  // bars. Dragging must be measured against the 800x450 content, not the
  // 800x800 element.
  const box = contentBox({ width: 800, height: 800 }, { width: 1920, height: 1080 })

  it('moves an overlay by the normalized share of the CONTENT box', () => {
    addOverlay(IMAGE)
    const overlay = overlaysState.overlays[0]
    overlay.x = 0
    overlay.y = 0

    dragOverlay(0, box, { dx: 200, dy: 45, dw: 0, dh: 0 })
    expect(overlay.x).toBeCloseTo(0.25)
    // 45px of a 450px tall content box, not of the 800px element.
    expect(overlay.y).toBeCloseTo(0.1)
  })

  it('resizes from a corner and pins the height once it is dragged', () => {
    addOverlay(IMAGE)
    const overlay = overlaysState.overlays[0]
    overlay.width = 0.25
    expect(overlay.height).toBeNull()

    dragOverlay(0, box, { dx: 0, dy: 0, dw: 80, dh: 45 })
    expect(overlay.width).toBeCloseTo(0.35)
    expect(overlay.height).not.toBeNull()
  })

  it('never lets a drag collapse a layer or push it out of range', () => {
    addOverlay(IMAGE)
    const overlay = overlaysState.overlays[0]
    dragOverlay(0, box, { dx: 0, dy: 0, dw: -10000, dh: 0 })
    expect(overlay.width).toBeCloseTo(0.01)

    dragOverlay(0, box, { dx: 100000, dy: 100000, dw: 0, dh: 0 })
    expect(overlay.x).toBe(2)
    expect(overlay.y).toBe(2)
  })

  it('moves a title anchor and leaves its alignment alone', () => {
    addTitle()
    const title = overlaysState.titles[0]
    title.x = 0.5
    title.y = 0.5
    title.align = 'right'

    dragTitle(0, box, { dx: -80, dy: 90, dw: 0, dh: 0 })
    expect(title.x).toBeCloseTo(0.4)
    expect(title.y).toBeCloseTo(0.7)
    expect(title.align).toBe('right')
  })

  it('is a no-op on an index that no longer exists', () => {
    dragOverlay(3, box, { dx: 10, dy: 10, dw: 0, dh: 0 })
    dragTitle(3, box, { dx: 10, dy: 10, dw: 0, dh: 0 })
    expect(overlaysState.overlays).toHaveLength(0)
  })

  it('survives an unmeasured box without producing NaN', () => {
    addTitle()
    const title = overlaysState.titles[0]
    dragTitle(0, { left: 0, top: 0, width: 0, height: 0 }, { dx: 10, dy: 10, dw: 0, dh: 0 })
    expect(Number.isFinite(title.x)).toBe(true)
    expect(Number.isFinite(title.y)).toBe(true)
  })
})

describe('chroma key eyedropper', () => {
  it('stores a sampled colour and turns the key on', () => {
    addOverlay(CLIP)
    startEyedropper(0)
    expect(overlaysUi.eyedropper).toBe(0)

    applyEyedropper('#12ab34')
    expect(overlaysState.overlays[0].chromaKey).toEqual({
      color: '#12AB34',
      similarity: 0.12,
      blend: 0.05,
    })
    expect(overlaysUi.eyedropper).toBeNull()
  })

  it('keeps the existing tolerances when re-picking a colour', () => {
    addOverlay(CLIP)
    overlaysState.overlays[0].chromaKey = { color: '#00FF00', similarity: 0.3, blend: 0.2 }
    startEyedropper(0)
    applyEyedropper('#0000FF')
    expect(overlaysState.overlays[0].chromaKey).toEqual({
      color: '#0000FF',
      similarity: 0.3,
      blend: 0.2,
    })
  })

  it('ignores a malformed sample and an idle eyedropper', () => {
    addOverlay(CLIP)
    startEyedropper(0)
    applyEyedropper('not a colour')
    expect(overlaysState.overlays[0].chromaKey?.color).toBe('#00FF00')

    applyEyedropper('#FFFFFF')
    expect(overlaysState.overlays[0].chromaKey?.color).toBe('#00FF00')
    startEyedropper(9)
    expect(overlaysUi.eyedropper).toBeNull()
  })
})

describe('subtitle cues', () => {
  const SRT = [
    '1',
    '00:00:01,000 --> 00:00:03,500',
    'Первая строка',
    'вторая строка',
    '',
    '2',
    '00:00:04,000 --> 00:00:06,000',
    'Вторая реплика',
    '',
  ].join('\r\n')

  it('parses SubRip with multi-line payloads and CRLF endings', () => {
    const cues = parseSubtitleCues(SRT)
    expect(cues).toEqual([
      { start: 1, end: 3.5, text: 'Первая строка\nвторая строка' },
      { start: 4, end: 6, text: 'Вторая реплика' },
    ])
  })

  it('parses WebVTT and ignores cue settings after the end stamp', () => {
    const vtt = 'WEBVTT\n\n00:01.000 --> 00:03.000 line:90% align:start\nHello\n'
    expect(parseSubtitleCues(vtt)).toEqual([{ start: 1, end: 3, text: 'Hello' }])
  })

  it('skips broken cues instead of throwing', () => {
    const broken = [
      '00:00:0x,000 --> 00:00:03,000',
      'bad start',
      '',
      '00:00:05,000 --> 00:00:03,000',
      'ends too early',
      '',
      '00:00:07,000 --> 00:00:08,000',
      'good',
      '',
    ].join('\n')
    expect(parseSubtitleCues(broken)).toEqual([{ start: 7, end: 8, text: 'good' }])
    expect(parseSubtitleCues('not a subtitle file')).toEqual([])
  })

  it('round-trips a cue list through SubRip', () => {
    const cues = parseSubtitleCues(SRT)
    const serialized = serializeSubtitleCues(cues)
    expect(serialized).toBe(
      '1\n00:00:01,000 --> 00:00:03,500\nПервая строка\nвторая строка\n\n' +
        '2\n00:00:04,000 --> 00:00:06,000\nВторая реплика\n',
    )
    expect(parseSubtitleCues(serialized)).toEqual(cues)
  })

  it('renumbers and drops empty cues on serialization', () => {
    const serialized = serializeSubtitleCues([
      { start: 0, end: 1, text: '  ' },
      { start: 2, end: 3, text: 'Есть текст' },
    ])
    expect(serialized.startsWith('1\n00:00:02,000 --> 00:00:03,000\n')).toBe(true)
  })

  it('clamps an inverted or non-finite cue instead of emitting garbage', () => {
    const serialized = serializeSubtitleCues([
      { start: 5, end: 1, text: 'обратный' },
      { start: Number.NaN, end: Number.POSITIVE_INFINITY, text: 'мусор' },
    ])
    expect(serialized).toContain('00:00:05,000 --> 00:00:05,000')
    // A non-finite pair collapses to a zero-length cue rather than NaN text.
    expect(serialized).toContain('00:00:00,000 --> 00:00:00,000')
  })

  it('finds the cue under the playhead', () => {
    setSubtitleCues(SUBTITLE_ID, parseSubtitleCues(SRT))
    expect(cueIndexAt(2)).toBe(0)
    expect(cueIndexAt(3.7)).toBe(-1)
    expect(cueIndexAt(5)).toBe(1)
    expect(cueIndexAt(Number.NaN)).toBe(-1)
  })

  it('marks the list dirty once it stops matching the stored asset', () => {
    attachSubtitles(SUBTITLE_ID)
    setSubtitleCues(SUBTITLE_ID, parseSubtitleCues(SRT))
    expect(overlaysUi.cuesDirty).toBe(false)

    overlaysUi.cues[0].text = 'Правка'
    markCuesDirty()
    expect(overlaysUi.cuesDirty).toBe(true)

    removeCue(1)
    expect(overlaysUi.cues).toHaveLength(1)
  })

  it('reports an empty document instead of silently attaching nothing', () => {
    setSubtitleCues(SUBTITLE_ID, [])
    expect(overlaysUi.cuesError).not.toBe('')
  })

  it('clears the cue list when the track is detached', () => {
    attachSubtitles(SUBTITLE_ID)
    setSubtitleCues(SUBTITLE_ID, parseSubtitleCues(SRT))
    clearSubtitles()
    expect(overlaysState.subtitles).toBeNull()
    expect(overlaysUi.cues).toEqual([])
    expect(overlaysUi.cuesAssetId).toBeNull()
  })
})

describe('snapshot restore', () => {
  it('drops a stale selection and stale cues', () => {
    addTitle()
    attachSubtitles(SUBTITLE_ID)
    setSubtitleCues(SUBTITLE_ID, [{ start: 0, end: 1, text: 'Старое' }])
    selectItem('title', 0)

    applyOverlaysSnapshot({ titles: [], overlays: [], subtitles: null })
    expect(overlaysUi.selection).toBeNull()
    expect(overlaysUi.cues).toEqual([])
  })

  it('keeps the cue list when the same asset comes back', () => {
    attachSubtitles(SUBTITLE_ID)
    setSubtitleCues(SUBTITLE_ID, [{ start: 0, end: 1, text: 'Старое' }])

    applyOverlaysSnapshot({ subtitles: { assetId: SUBTITLE_ID, burnIn: true } })
    expect(overlaysUi.cues).toHaveLength(1)
  })

  it('restores hostile JSON into clamped state', () => {
    applyOverlaysSnapshot({
      titles: [{ text: 'A', fontSize: 1e9, x: Number.NaN, color: 'rgb(1,2,3)' }],
      overlays: [{ assetId: IMAGE.id, kind: 'image', width: 'wide', opacity: 12 }],
    })
    const title = overlaysState.titles[0]
    expect(title.fontSize).toBe(512)
    expect(title.x).toBe(0.5)
    expect(title.color).toBe('#FFFFFF')

    const overlay = overlaysState.overlays[0]
    expect(overlay.width).toBe(0.25)
    expect(overlay.opacity).toBe(1)
  })
})
