import { describe, expect, it } from 'vitest'

import {
  createProjectEnvelope,
  migrateProjectDocument,
  nextProjectRevision,
  PROJECT_DOCUMENT_SCHEMA_VERSION,
  updateLegacyProjectValues,
} from './project-schema'

describe('project document schema', () => {
  it('migrates v1 to v3 and preserves unknown legacy fields', () => {
    const document = migrateProjectDocument({
      schemaVersion: 1,
      videoId: 'video-1',
      name: 'Demo',
      video: { id: 'video-1', duration: 12.5, width: 1920, height: 1080 },
      edit: { filter: 'sepia' },
      pluginState: { revision: 7 },
    })

    expect(document.schemaVersion).toBe(PROJECT_DOCUMENT_SCHEMA_VERSION)
    expect(document.sequences[0]?.tracks).toHaveLength(2)
    expect(document.sequences[0]?.tracks[0]?.clips[0]?.durationTicks).toBe(12_500_000)
    expect(document.legacyFields?.pluginState).toEqual({ revision: 7 })
  })

  it('migrates v2 asset identity to v3 without changing media order or extensions', () => {
    const fingerprint = 'ab'.repeat(32)
    const document = migrateProjectDocument({
      schemaVersion: 2,
      name: 'v2', primaryMediaId: 'a', activeSequenceId: 'sequence-main',
      pluginTop: { keep: true },
      media: [
        { id: 'a', kind: 'video', pluginMedia: 1, metadata: {
          duration: 1, assetId: 'asset-a', fingerprint, url: 'blob:runtime',
        } },
        { id: 'b', kind: 'video', pluginMedia: 2, metadata: { duration: 1 } },
      ],
      sequences: [{ id: 'sequence-main', name: 'Main', settings: { timeBase: 1_000_000 }, tracks: [] }],
    })
    expect(document.schemaVersion).toBe(3)
    expect(document.media.map((media) => media.id)).toEqual(['a', 'b'])
    expect(document.media[0]).toMatchObject({ assetRef: 'asset-a', contentFingerprint: fingerprint, pluginMedia: 1 })
    expect(document.media[1]).toMatchObject({ assetRef: 'b', pluginMedia: 2 })
    expect(document.media[0]!.metadata).not.toHaveProperty('url')
    expect(document.media[0]!.metadata).not.toHaveProperty('assetId')
    expect(document.pluginTop).toEqual({ keep: true })
  })

  it('creates an audio primary asset on an audio track', () => {
    const document = migrateProjectDocument({
      videoId: 'audio-1',
      video: {
        id: 'audio-1', filename: 'voice.wav', duration: 3,
        width: 0, height: 0, mediaKind: 'audio', acodec: 'pcm_s16le',
      },
      edit: {},
    })
    expect(document.media[0]).toMatchObject({ id: 'audio-1', kind: 'audio' })
    expect(document.sequences[0]!.tracks.find((track) => track.kind === 'video')!.clips)
      .toHaveLength(0)
    expect(document.sequences[0]!.tracks.find((track) => track.kind === 'audio')!.clips[0])
      .toMatchObject({ id: 'clip-main', mediaId: 'audio-1' })
  })

  it('round-trips unknown v2 fields and rejects a forward version', () => {
    const document = migrateProjectDocument({
      schemaVersion: 1,
      videoId: 'video-1',
      video: { id: 'video-1', duration: 1 },
      edit: {},
    })
    const value = JSON.parse(JSON.stringify(document)) as Record<string, unknown>
    value.pluginTop = { enabled: true }
    const decoded = migrateProjectDocument(value)

    expect(decoded.pluginTop).toEqual({ enabled: true })
    expect(() => migrateProjectDocument({ schemaVersion: 4 })).toThrow(
      'unsupported project schemaVersion 4',
    )
  })

  it('never persists runtime media locators in the canonical document', () => {
    const document = migrateProjectDocument({
      videoId: 'video-1',
      video: {
        id: 'video-1', filename: 'clip.mp4', duration: 1,
        url: 'blob:old-tab', path: '/private/source.mp4', assetId: 'video-1',
      },
      edit: {},
    })
    expect(document.media[0]).toMatchObject({ assetRef: 'video-1' })
    expect(document.media[0]!.metadata).not.toHaveProperty('assetId')
    expect(document.media[0]!.metadata).not.toHaveProperty('url')
    expect(document.media[0]!.metadata).not.toHaveProperty('path')
    const savedV2 = structuredClone(document)
    savedV2.media[0]!.metadata.url = 'blob:v2-primary'
    savedV2.media[0]!.metadata.availability = 'ready'
    savedV2.media.push({
      id: 'secondary', kind: 'video',
      metadata: { duration: 2, url: 'blob:v2-secondary', path: '/tmp/secondary.mp4' },
    })
    const reopened = migrateProjectDocument(savedV2)
    expect(reopened.media.every((media) =>
      !('url' in media.metadata) && !('path' in media.metadata) && !('availability' in media.metadata),
    )).toBe(true)
    const updated = updateLegacyProjectValues(
      savedV2, 'Updated', { url: 'blob:new-tab', duration: 1 }, {},
    )
    expect(updated.media[0]!.metadata).not.toHaveProperty('url')
    expect(updated.media[0]!.metadata).not.toHaveProperty('availability')
  })

  it('keeps project identity while advancing persistence revision', () => {
    const document = migrateProjectDocument({
      videoId: 'video-1',
      video: { id: 'video-1', duration: 1 },
      edit: {},
    })
    const first = createProjectEnvelope('project-1', document, 10)
    const second = nextProjectRevision(first, document, 20)

    expect(second).toMatchObject({
      projectId: 'project-1',
      revision: 2,
      createdAt: 10,
      updatedAt: 20,
    })
  })

  it('patches legacy values without losing nested unknown fields', () => {
    const document = migrateProjectDocument({
      videoId: 'video-1',
      video: { id: 'video-1', duration: 1, vendorProbe: { keep: true } },
      edit: { filter: 'sepia', vendorEffect: [1, 2] },
    })
    document.sequences[0]!.settings.vendorSettings = null
    document.sequences[0]!.tracks[0]!.vendorTrack = { keep: true }
    const updated = updateLegacyProjectValues(
      document,
      'Updated',
      { id: 'video-1', duration: 1 },
      { filter: 'warm' },
    )
    const effect = updated.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!
    expect(updated.media[0]!.metadata.vendorProbe).toEqual({ keep: true })
    expect(effect.parameters.vendorEffect).toEqual([1, 2])
    expect(updated.sequences[0]!.settings.vendorSettings).toBeNull()
    expect(updated.sequences[0]!.tracks[0]!.vendorTrack).toEqual({ keep: true })
  })

  it('does not silently retarget durable media identity during legacy field updates', () => {
    const fingerprint = 'a'.repeat(64)
    const document = migrateProjectDocument({
      videoId: 'clip-1', video: { id: 'clip-1', assetId: 'asset-original', fingerprint }, edit: {},
    })
    const updated = updateLegacyProjectValues(document, 'Same source', {
      id: 'clip-1', assetId: 'asset-other', fingerprint: 'b'.repeat(64), duration: 2,
    }, {})
    expect(updated.media[0]).toMatchObject({
      assetRef: 'asset-original', contentFingerprint: fingerprint,
      metadata: expect.objectContaining({ duration: 2 }),
    })
  })

  it('matches Rust boundary validation for dimensions, timebase and ranges', () => {
    const value = migrateProjectDocument({
      videoId: 'video-1',
      video: { id: 'video-1', duration: 1 },
      edit: {},
    })
    const invalidWidth = structuredClone(value)
    invalidWidth.sequences[0]!.settings.width = -1
    expect(() => migrateProjectDocument(invalidWidth)).toThrow('invalid sequence dimensions')
    const invalidTimebase = structuredClone(value)
    invalidTimebase.sequences[0]!.settings.timeBase = 0x1_0000_0000
    expect(() => migrateProjectDocument(invalidTimebase)).toThrow('invalid project sequence')
    const invalidRange = structuredClone(value)
    invalidRange.sequences[0]!.tracks[0]!.clips[0]!.sourceOutTick = 2
    expect(() => migrateProjectDocument(invalidRange)).toThrow('invalid project clip range')
  })

  it('rejects source overflow, incompatible tracks and same-track overlap', () => {
    const value = migrateProjectDocument({
      videoId: 'video-1',
      video: { id: 'video-1', duration: 1 },
      edit: {},
    })

    const sourceOverflow = structuredClone(value)
    const overflowClip = sourceOverflow.sequences[0]!.tracks[0]!.clips[0]!
    overflowClip.durationTicks = 2_000_000
    overflowClip.sourceOutTick = 2_000_000
    expect(() => migrateProjectDocument(sourceOverflow)).toThrow(
      'invalid project clip source range',
    )

    const incompatible = structuredClone(value)
    incompatible.media[0]!.kind = 'audio'
    expect(() => migrateProjectDocument(incompatible)).toThrow(
      'incompatible project media and track',
    )

    const overlap = structuredClone(value)
    const track = overlap.sequences[0]!.tracks[0]!
    track.clips.push({
      ...structuredClone(track.clips[0]!),
      id: 'clip-overlap',
      effects: [],
    })
    expect(() => migrateProjectDocument(overlap)).toThrow('overlapping project clips')
  })
})
