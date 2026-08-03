import { describe, expect, it } from 'vitest'

import type { ProjectClip, ProjectDocument, ProjectTrack } from '../project-schema'
import { migrateProjectDocument } from '../project-schema'
import { applyTimelineCommand, StructuralHistory } from './timeline'

function fixture(): ProjectDocument {
  const document = migrateProjectDocument({
    videoId: 'media-video-0',
    name: '8 tracks / 20 clips',
    video: { id: 'media-video-0', duration: 120 },
    edit: {},
  })
  document.media = [
    ...Array.from({ length: 4 }, (_, index) => ({
      id: `media-video-${index}`,
      kind: 'video',
      metadata: { duration: 120, fps: [23.976, 25, 29.97, 59.94][index] },
    })),
    ...Array.from({ length: 4 }, (_, index) => ({
      id: `media-audio-${index}`,
      kind: 'audio',
      metadata: { duration: 120, sampleRate: index % 2 ? 44_100 : 48_000 },
    })),
  ]
  const tracks: ProjectTrack[] = Array.from({ length: 8 }, (_, index) => ({
    id: `track-${index}`,
    kind: index < 4 ? 'video' : 'audio',
    name: `Track ${index}`,
    clips: [],
    muted: false,
    solo: false,
    locked: false,
    hidden: false,
  }))
  for (let index = 0; index < 20; index++) {
    const trackIndex = index % 8
    const mediaIndex = trackIndex % 4
    const clip: ProjectClip = {
      id: `clip-${index}`,
      mediaId: trackIndex < 4 ? `media-video-${mediaIndex}` : `media-audio-${mediaIndex}`,
      timelineStartTick: Math.floor(index / 8) * 4_000_000 + (index % 4) * 500_000,
      durationTicks: 3_000_000,
      sourceInTick: index * 10_000,
      sourceOutTick: index * 10_000 + 3_000_000,
      effects: [],
    }
    tracks[trackIndex]!.clips.push(clip)
  }
  document.sequences[0]!.tracks = tracks
  return migrateProjectDocument(structuredClone(document))
}

describe('canonical timeline commands', () => {
  it('round-trips an 8-track 20-clip fixture with overlaps and stable IDs', () => {
    const document = fixture()
    const reopened = migrateProjectDocument(JSON.parse(JSON.stringify(document)))
    expect(reopened).toEqual(document)
    expect(reopened.sequences[0]!.tracks).toHaveLength(8)
    expect(reopened.sequences[0]!.tracks.flatMap((track) => track.clips)).toHaveLength(20)
    expect(reopened.sequences[0]!.tracks[1]!.clips[0]!.timelineStartTick).toBeLessThan(
      reopened.sequences[0]!.tracks[0]!.clips[0]!.timelineStartTick +
        reopened.sequences[0]!.tracks[0]!.clips[0]!.durationTicks,
    )
  })

  it('moves across compatible tracks and trims without changing source metadata', () => {
    const document = fixture()
    const metadata = structuredClone(document.media[0]!.metadata)
    const moved = applyTimelineCommand(document, {
      kind: 'move_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      targetTrackId: 'track-1',
      targetIndex: 0,
      timelineStartTick: 12_000_000,
    })
    const trimmed = applyTimelineCommand(moved, {
      kind: 'trim_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      sourceInTick: 500_000,
      sourceOutTick: 2_500_000,
      timelineStartTick: 12_500_000,
    })
    const clip = trimmed.sequences[0]!.tracks[1]!.clips.find((item) => item.id === 'clip-0')!
    expect(clip).toMatchObject({
      sourceInTick: 500_000,
      sourceOutTick: 2_500_000,
      durationTicks: 2_000_000,
      timelineStartTick: 12_500_000,
    })
    expect(trimmed.media[0]!.metadata).toEqual(metadata)
    expect(document.sequences[0]!.tracks[0]!.clips.some((item) => item.id === 'clip-0')).toBe(true)
  })

  it('undoes and redoes structural commands exactly and clears redo on a branch', () => {
    const initial = fixture()
    const history = new StructuralHistory()
    const moved = history.execute(initial, {
      kind: 'move_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      targetTrackId: 'track-1',
      targetIndex: 0,
      timelineStartTick: 12_000_000,
    })
    const trimmed = history.execute(moved, {
      kind: 'trim_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      sourceInTick: 100_000,
      sourceOutTick: 2_900_000,
      timelineStartTick: 12_100_000,
    })
    expect(history.undo(history.undo(trimmed))).toEqual(initial)
    const movedAgain = history.redo(initial)
    expect(movedAgain).toEqual(moved)
    const branch = history.execute(movedAgain, {
      kind: 'set_track_state',
      sequenceId: 'sequence-main',
      trackId: 'track-1',
      patch: { muted: true },
    })
    expect(branch.sequences[0]!.tracks[1]!.muted).toBe(true)
    expect(history.canRedo).toBe(false)
  })

  it('rejects locked or incompatible targets atomically without a history entry', () => {
    const initial = fixture()
    initial.sequences[0]!.tracks[1]!.locked = true
    const history = new StructuralHistory()
    expect(() =>
      history.execute(initial, {
        kind: 'move_clip',
        sequenceId: 'sequence-main',
        clipId: 'clip-0',
        targetTrackId: 'track-1',
        targetIndex: 0,
        timelineStartTick: 1,
      }),
    ).toThrow('locked')
    expect(history.canUndo).toBe(false)
    expect(initial.sequences[0]!.tracks[0]!.clips.some((clip) => clip.id === 'clip-0')).toBe(true)

    expect(() =>
      applyTimelineCommand(initial, {
        kind: 'move_clip',
        sequenceId: 'sequence-main',
        clipId: 'clip-0',
        targetTrackId: 'track-4',
        targetIndex: 0,
        timelineStartTick: 1,
      }),
    ).toThrow('incompatible')
  })

  it('rejects a failed compound edit without publishing partial removals', () => {
    const initial = fixture()
    const history = new StructuralHistory()
    expect(() =>
      history.execute(initial, {
        kind: 'batch',
        commands: [
          {
            kind: 'remove_clip',
            sequenceId: 'sequence-main',
            trackId: 'track-0',
            clipId: 'clip-0',
          },
          {
            kind: 'insert_clip',
            sequenceId: 'sequence-main',
            trackId: 'track-4',
            index: 0,
            clip: structuredClone(initial.sequences[0]!.tracks[0]!.clips[0]!),
          },
        ],
      }),
    ).toThrow('incompatible')
    expect(initial.sequences[0]!.tracks[0]!.clips.some((clip) => clip.id === 'clip-0')).toBe(true)
    expect(history.canUndo).toBe(false)
  })

  it('atomically inserts a new media asset and deduplicates it on another clip', () => {
    const initial = fixture()
    const media = { id: 'media-new', kind: 'video', metadata: { duration: 4, fps: 25 } }
    const firstClip: ProjectClip = {
      id: 'clip-new-1',
      mediaId: media.id,
      timelineStartTick: 12_000_000,
      durationTicks: 4_000_000,
      sourceInTick: 0,
      sourceOutTick: 4_000_000,
      effects: [],
    }
    const inserted = applyTimelineCommand(initial, {
      kind: 'insert_media_clip',
      sequenceId: 'sequence-main',
      trackId: 'track-0',
      index: initial.sequences[0]!.tracks[0]!.clips.length,
      media,
      clip: firstClip,
    })
    const insertedAgain = applyTimelineCommand(inserted, {
      kind: 'insert_media_clip',
      sequenceId: 'sequence-main',
      trackId: 'track-0',
      index: inserted.sequences[0]!.tracks[0]!.clips.length,
      media: { ...media, metadata: { duration: 4 } },
      clip: { ...firstClip, id: 'clip-new-2', timelineStartTick: 16_000_000 },
    })
    expect(insertedAgain.media.filter((item) => item.id === media.id)).toHaveLength(1)
    expect(insertedAgain.sequences[0]!.tracks[0]!.clips.filter(
      (clip) => clip.mediaId === media.id,
    )).toHaveLength(2)
    expect(() => applyTimelineCommand(inserted, {
      kind: 'insert_media_clip',
      sequenceId: 'sequence-main',
      trackId: 'track-0',
      index: inserted.sequences[0]!.tracks[0]!.clips.length,
      media: { ...media, metadata: { duration: 5 } },
      clip: { ...firstClip, id: 'clip-conflict', timelineStartTick: 16_000_000 },
    })).toThrow('conflicting media')

    expect(() => applyTimelineCommand(initial, {
      kind: 'insert_media_clip',
      sequenceId: 'sequence-main',
      trackId: 'track-4',
      index: 0,
      media,
      clip: firstClip,
    })).toThrow('incompatible')
    expect(initial.media.some((item) => item.id === media.id)).toBe(false)
    expect(() => applyTimelineCommand(initial, {
      kind: 'insert_media_clip',
      sequenceId: 'sequence-main',
      trackId: 'track-0',
      index: 1,
      media,
      clip: { ...firstClip, mediaId: 'media-video-0' },
    })).toThrow('identity mismatch')
  })

  it('keeps newer parameter values when undoing a structural edit', () => {
    const initial = fixture()
    initial.sequences[0]!.tracks[0]!.clips[0]!.effects.push({
      id: 'effect-legacy-edit',
      kind: 'legacy_edit',
      enabled: true,
      parameters: {},
    })
    const history = new StructuralHistory()
    const moved = history.execute(initial, {
      kind: 'move_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      targetTrackId: 'track-0',
      targetIndex: 0,
      timelineStartTick: 1_000_000,
    })
    const effect = moved.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!
    effect.parameters = { filter: 'sepia' }
    const undone = history.undo(moved)
    expect(undone.sequences[0]!.tracks[0]!.clips[0]!.timelineStartTick).toBe(0)
    expect(undone.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!.parameters).toEqual({
      filter: 'sepia',
    })
  })

  it('accepts server-normalized object key order when undoing', () => {
    const initial = fixture()
    const history = new StructuralHistory()
    const moved = history.execute(initial, {
      kind: 'move_clip',
      sequenceId: 'sequence-main',
      clipId: 'clip-0',
      targetTrackId: 'track-0',
      targetIndex: 0,
      timelineStartTick: 1_000_000,
    })
    moved.sequences[0]!.tracks = moved.sequences[0]!.tracks.map((track) => ({
      hidden: track.hidden,
      locked: track.locked,
      muted: track.muted,
      solo: track.solo,
      clips: track.clips,
      name: track.name,
      kind: track.kind,
      id: track.id,
    }))
    expect(history.undo(moved).sequences[0]!.tracks[0]!.clips[0]!.timelineStartTick).toBe(0)
  })

  it('groups pointer-drag updates and keeps history within its measured byte budget', () => {
    const initial = fixture()
    const singleEntryBytes = JSON.stringify(initial).length * 3
    const history = new StructuralHistory(singleEntryBytes)
    let current = initial
    for (let index = 0; index < 10; index++) {
      current = history.execute(
        current,
        {
          kind: 'move_clip',
          sequenceId: 'sequence-main',
          clipId: 'clip-0',
          targetTrackId: 'track-0',
          targetIndex: 0,
          timelineStartTick: index + 1,
        },
        index < 3 ? 'drag-1' : undefined,
      )
    }
    expect(history.bytes).toBeLessThanOrEqual(singleEntryBytes)
    expect(history.canUndo).toBe(true)

    const tiny = new StructuralHistory(1)
    expect(() =>
      tiny.execute(initial, {
        kind: 'set_track_state',
        sequenceId: 'sequence-main',
        trackId: 'track-0',
        patch: { muted: true },
      }),
    ).toThrow('budget exceeded')
    expect(tiny.bytes).toBe(0)
    expect(tiny.canUndo).toBe(false)
  })
})
