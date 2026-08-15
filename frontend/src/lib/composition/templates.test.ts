import { describe, expect, it } from 'vitest'

import {
  createCompositionTemplate,
  instantiateCompositionTemplate,
  parseCompositionTemplate,
  serializeCompositionTemplate,
} from './templates'
import { compositionMarkers, upsertCompositionMarker } from './markers'
import type { Composition } from './types'

function fixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: 1_000_000,
    canvas: { width: 1280, height: 720, fps: 30, backgroundColor: '#000000' },
    sources: {
      original: {
        id: 'original',
        kind: 'video',
        durationTicks: 5_000_000,
        width: 1280,
        height: 720,
        hasAudio: true,
      },
    },
    tracks: [
      {
        id: 'video-main',
        kind: 'video',
        name: 'Main',
        hidden: false,
        muted: false,
        locked: false,
        clips: [
          {
            id: 'video-slot-clip',
            kind: 'video',
            sourceId: 'original',
            timelineStartTicks: 0,
            sourceInTicks: 0,
            sourceOutTicks: 2_000_000,
            transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
            opacity: 1,
            sourceAudioEnabled: true,
            audioGain: 1,
          },
        ],
      },
      {
        id: 'text-main',
        kind: 'text',
        name: 'Title',
        hidden: false,
        locked: false,
        clips: [
          {
            id: 'title-slot-clip',
            kind: 'text',
            timelineStartTicks: 0,
            durationTicks: 2_000_000,
            text: 'Placeholder',
            x: 100,
            y: 100,
            opacity: 1,
            style: { fontSizePx: 48, color: '#FFFFFF', align: 'center' },
          },
        ],
      },
    ],
  }
}

describe('deterministic composition templates', () => {
  it('replaces declared media and text slots without retaining unused sources', () => {
    const template = createCompositionTemplate('promo-template', 'Promo', fixture(), [
      { id: 'hero', kind: 'media', label: 'Hero video', clipId: 'video-slot-clip' },
      { id: 'title', kind: 'text', label: 'Title', clipId: 'title-slot-clip' },
    ])
    const result = instantiateCompositionTemplate(template, {
      hero: {
        source: {
          id: 'replacement',
          kind: 'video',
          durationTicks: 3_000_000,
          width: 1920,
          height: 1080,
          hasAudio: true,
        },
      },
      title: { text: 'Launch day' },
    })
    expect(Object.keys(result.sources)).toEqual(['replacement'])
    expect(result.tracks[0]!.clips[0]).toMatchObject({ sourceId: 'replacement' })
    expect(result.tracks[1]!.clips[0]).toMatchObject({ text: 'Launch day' })
  })

  it('round-trips a path-free JSON package', () => {
    const marked = upsertCompositionMarker(fixture(), {
      id: 'template-beat',
      tick: 750_000,
      label: 'Beat',
      color: '#38bdf8',
    })
    const template = createCompositionTemplate('plain-template', 'Plain', marked, [])
    const encoded = serializeCompositionTemplate(template)
    expect(encoded).not.toContain('path')
    const parsed = parseCompositionTemplate(encoded)
    expect(parsed).toEqual(template)
    expect(compositionMarkers(instantiateCompositionTemplate(parsed, {}))).toEqual([
      { id: 'template-beat', tick: 750_000, label: 'Beat', color: '#38bdf8' },
    ])
  })

  it('preserves authoring-only multicam groups and their otherwise-unused angle sources', () => {
    const base = fixture()
    const authored: Composition = {
      ...base,
      sources: {
        ...base.sources,
        backup: {
          id: 'backup',
          kind: 'video',
          durationTicks: 4_000_000,
          width: 1280,
          height: 720,
          hasAudio: true,
        },
      },
      multicamGroups: [{
        id: 'template-multicam',
        name: 'Template multicam',
        timelineStartTicks: 0,
        durationTicks: 2_000_000,
        videoTrackId: 'video-main',
        angles: [
          { id: 'template-angle-a', label: 'A', sourceId: 'original', sourceTickAtGroupStart: 0 },
          { id: 'template-angle-b', label: 'B', sourceId: 'backup', sourceTickAtGroupStart: 0 },
        ],
        switches: [{
          id: 'template-switch',
          clipId: 'video-slot-clip',
          timelineTick: 0,
          angleId: 'template-angle-a',
        }],
      }],
    }
    const template = createCompositionTemplate('multicam-template', 'Multicam', authored, [])
    const instantiated = instantiateCompositionTemplate(
      parseCompositionTemplate(serializeCompositionTemplate(template)),
      {},
    )

    expect(instantiated.multicamGroups).toEqual(authored.multicamGroups)
    expect(Object.keys(instantiated.sources).sort()).toEqual(['backup', 'original'])
  })

  it('preserves compatible unknown document, track, and clip fields through round-trip and instantiate', () => {
    const authored = fixture() as Composition & Record<string, unknown>
    authored.futureDocumentData = { revision: 9 }
    ;(authored.tracks[0] as unknown as Record<string, unknown>).futureTrackData = ['blend-stack']
    ;(authored.tracks[0]!.clips[0] as unknown as Record<string, unknown>).futureClipData = { motion: true }
    ;(authored.tracks[0]!.clips[0] as unknown as Record<string, unknown>).playbackMode = { mode: 'reverse' }
    ;(authored.tracks[0]!.clips[0] as unknown as Record<string, unknown>).stabilization = {
      mode: 'deshake',
      radiusX: 32,
      radiusY: 48,
    }
    ;(authored.tracks[0]!.clips[0] as unknown as Record<string, unknown>).animation = {
      x: {
        mode: 'keyframes',
        track: {
          timeBase: 1_000_000,
          interpolation: 'linear',
          keyframes: [{ tick: 0, value: 0 }, { tick: 1_000_000, value: 120 }],
        },
      },
    }
    ;(authored.tracks[0]!.clips[0] as unknown as Record<string, unknown>).masks = [{
      id: 'template-mask',
      shape: 'ellipse',
      x: { mode: 'constant', value: 0.5 },
      y: { mode: 'constant', value: 0.5 },
      width: { mode: 'constant', value: 0.8 },
      height: { mode: 'constant', value: 0.6 },
      feather: 0.1,
      inverted: false,
    }]

    const template = createCompositionTemplate('future-template', 'Future safe', authored, [])
    const parsed = parseCompositionTemplate(serializeCompositionTemplate(template))
    const instantiated = instantiateCompositionTemplate(parsed, {})

    expect((instantiated as unknown as Record<string, unknown>).futureDocumentData).toEqual({ revision: 9 })
    expect((instantiated.tracks[0] as unknown as Record<string, unknown>).futureTrackData).toEqual(['blend-stack'])
    expect((instantiated.tracks[0]!.clips[0] as unknown as Record<string, unknown>).futureClipData).toEqual({ motion: true })
    expect(instantiated.tracks[0]!.clips[0]).toMatchObject({
      animation: { x: { mode: 'keyframes', track: { keyframes: [{ tick: 0 }, { tick: 1_000_000 }] } } },
      masks: [{ id: 'template-mask', shape: 'ellipse' }],
      playbackMode: { mode: 'reverse' },
      stabilization: { mode: 'deshake', radiusX: 32, radiusY: 48 },
    })
  })

  it('rejects missing or incompatible replacements', () => {
    const template = createCompositionTemplate('typed-template', 'Typed', fixture(), [
      { id: 'hero', kind: 'media', label: 'Hero', clipId: 'video-slot-clip' },
    ])
    expect(() => instantiateCompositionTemplate(template, {})).toThrow('Replacement is missing')
    expect(() =>
      instantiateCompositionTemplate(template, {
        hero: {
          source: {
            id: 'audio-only',
            kind: 'audio',
            durationTicks: 3_000_000,
            width: 0,
            height: 0,
            hasAudio: true,
          },
        },
      }),
    ).toThrow('incompatible')
  })
})
