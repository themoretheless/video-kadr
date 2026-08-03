import { describe, expect, it, vi } from 'vitest'
import { PreviewFrameCache, legacySingleClipPreviewEligible, previewFrameKey, previewGraphFingerprint, previewTimelineTick, sourceMediaTimeToEditedSeconds, type PreviewFrameIdentity } from './optimized-preview-cache'
import type { ProjectDocument } from './project-schema'

const source = 'a'.repeat(64)
const settings = { width: 1280, height: 720, pixelRatioMilli: 1000, sourceMode: 'proxy' as const, rendererCompatibility: 'canvas-v1-srgb' }
function identity(timelineTick = 1): PreviewFrameIdentity { return { sourceFingerprint: source, graphVersion: previewGraphFingerprint(source, { contrast: 1 }), timelineTick, settings } }
function bitmap(width: number, height: number) { return { width, height, close: vi.fn() } as unknown as ImageBitmap }

describe('optimized preview frame cache', () => {
  it('keys source, semantic graph, exact canonical tick and render settings', () => {
    expect(previewFrameKey(identity())).toBe(previewFrameKey(identity()))
    expect(previewFrameKey(identity(1))).not.toBe(previewFrameKey(identity(2)))
    expect(previewGraphFingerprint(source, { a: 1, b: 2 })).toBe(previewGraphFingerprint(source, { b: 2, a: 1 }))
    expect(previewGraphFingerprint(source, { contrast: 1 })).not.toBe(previewGraphFingerprint(source, { contrast: 2 }))
    expect(previewTimelineTick(1 / 3)).toBe(333_333)
  })

  it('maps raw source time through trim, concatenated segments, speed and reverse', () => {
    expect(sourceMediaTimeToEditedSeconds(7, 20, { trim: { start: 5, end: 15 }, speed: 2 })).toBe(1)
    expect(sourceMediaTimeToEditedSeconds(6, 20, {
      segments: [{ start: 0, end: 4 }, { start: 6, end: 10 }],
    })).toBe(4)
    expect(sourceMediaTimeToEditedSeconds(4, 20, {
      segments: [{ start: 0, end: 4 }, { start: 6, end: 10 }],
    })).toBeNull()
    expect(sourceMediaTimeToEditedSeconds(11, 20, {
      segments: [{ start: 10, end: 12 }, { start: 2, end: 5 }], speed: 2,
    })).toBe(2)
    expect(sourceMediaTimeToEditedSeconds(7, 20, {
      segments: [{ start: 2, end: 5 }, { start: 10, end: 12 }], speed: 1,
    })).toBeNull()
    expect(sourceMediaTimeToEditedSeconds(3, 10, { trim: { start: 2, end: 8 }, reverse: true, speed: 2 })).toBe(2.5)
  })

  it('allows backend exact frames only for an untouched legacy single-clip topology', () => {
    const document = {
      schemaVersion: 3, name: 'p', primaryMediaId: 'media', activeSequenceId: 'sequence', media: [{ id: 'media', kind: 'video', metadata: { duration: 0.00001 } }],
      sequences: [{ id: 'sequence', name: 's', settings: { timeBase: 1_000_000 }, tracks: [{
        id: 'track', kind: 'video', clips: [{ id: 'clip', mediaId: 'media', timelineStartTick: 0, sourceInTick: 0, sourceOutTick: 10, durationTicks: 10, effects: [{ id: 'fx', kind: 'legacy_edit', enabled: true, parameters: {} }] }],
      }] }],
    } as unknown as ProjectDocument
    expect(legacySingleClipPreviewEligible(document)).toBe(true)
    document.sequences[0]!.tracks[0]!.hidden = true
    expect(legacySingleClipPreviewEligible(document)).toBe(false)
    document.sequences[0]!.tracks[0]!.hidden = false
    document.sequences[0]!.tracks[0]!.clips[0]!.effects.push({ id: 'extra', kind: 'blur', enabled: true, parameters: {} })
    expect(legacySingleClipPreviewEligible(document)).toBe(false)
    document.sequences[0]!.tracks[0]!.clips[0]!.effects.pop()
    document.sequences[0]!.tracks[0]!.clips[0]!.timelineStartTick = 1
    expect(legacySingleClipPreviewEligible(document)).toBe(false)
    document.sequences[0]!.tracks[0]!.clips.push({ ...document.sequences[0]!.tracks[0]!.clips[0]!, id: 'secondary' })
    expect(legacySingleClipPreviewEligible(document)).toBe(false)
  })

  it('enforces a byte-weighted LRU and closes every evicted bitmap', () => {
    const cache = new PreviewFrameCache(32, 2)
    const a = bitmap(2, 2), b = bitmap(2, 2), c = bitmap(2, 2)
    cache.put('a', a); cache.put('b', b); cache.get('a'); cache.put('c', c)
    expect(cache.get('b')).toBeNull(); expect((b.close as ReturnType<typeof vi.fn>)).toHaveBeenCalledOnce()
    expect(cache.size).toBe(2); expect(cache.bytes).toBe(32)
    cache.clear(); expect(cache.size).toBe(0); expect(cache.bytes).toBe(0)
  })

  it('rejects a single decoded frame larger than the complete budget', () => {
    const cache = new PreviewFrameCache(8, 2), frame = bitmap(2, 2)
    expect(cache.put('oversized', frame)).toBe(false)
    expect((frame.close as ReturnType<typeof vi.fn>)).toHaveBeenCalledOnce()
  })
})
