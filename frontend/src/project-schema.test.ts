import { describe, expect, it } from 'vitest'

import {
  createProjectEnvelope,
  migrateProjectDocument,
  nextProjectRevision,
  PROJECT_DOCUMENT_SCHEMA_VERSION,
  updateLegacyProjectValues,
} from './project-schema'

describe('project document schema', () => {
  it('migrates v1 to v2 and preserves unknown legacy fields', () => {
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
    expect(() => migrateProjectDocument({ schemaVersion: 3 })).toThrow(
      'unsupported project schemaVersion 3',
    )
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
