import { describe, expect, it } from 'vitest'

import type { ProjectClip, ProjectDocument, ProjectTrack } from '../project-schema'
import { migrateProjectDocument } from '../project-schema'
import { applyTimelineCommand, getClipTransition, projectTrackGaps, rippleDeleteClip, splitClipAt, StructuralHistory } from './timeline'

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
  it('attaches, validates, splits away and removes clip transitions', () => {
    const document = fixture()
    const sequenceId = document.sequences[0]!.id
    const moved = applyTimelineCommand(document, {
      kind: 'move_clip', sequenceId, clipId: 'clip-8',
      targetTrackId: 'track-0', targetIndex: 1, timelineStartTick: 3_000_000,
    })
    expect(getClipTransition(moved.sequences[0]!.tracks[0]!.clips[1]!)).toBeNull()
    const withTransition = applyTimelineCommand(moved, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-8',
      transition: { type: 'crossfade', durationTicks: 50_000 },
    })
    expect(getClipTransition(withTransition.sequences[0]!.tracks[0]!.clips[1]!))
      .toEqual({ type: 'crossfade', durationTicks: 50_000 })
    expect(() => applyTimelineCommand(withTransition, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-8',
      transition: { type: 'crossfade', durationTicks: 100_000 },
    })).toThrow('source handles')
    expect(() => applyTimelineCommand(withTransition, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-16',
      transition: { type: 'fade-black', durationTicks: 10_000 },
    })).toThrow('adjacent')
    const split = applyTimelineCommand(withTransition, splitClipAt(withTransition, sequenceId, 'clip-8', 4_000_000))
    const [splitLeft, splitRight] = split.sequences[0]!.tracks[0]!.clips.slice(1, 3)
    expect(getClipTransition(splitLeft!)).toBeNull()
    expect(getClipTransition(splitRight!)).toBeNull()
    const wiped = applyTimelineCommand(moved, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-8',
      transition: { type: 'wipe-left', durationTicks: 50_000 },
    })
    expect(getClipTransition(wiped.sequences[0]!.tracks[0]!.clips.find((clip) => clip.id === 'clip-8')!))
      .toEqual({ type: 'wipe-left', durationTicks: 50_000 })
    expect(() => applyTimelineCommand(moved, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-8',
      transition: { type: 'dissolve' as never, durationTicks: 50_000 },
    })).toThrow('invalid transition type')
    const removed = applyTimelineCommand(withTransition, {
      kind: 'set_clip_transition', sequenceId, trackId: 'track-0', clipId: 'clip-8',
      transition: null,
    })
    expect(getClipTransition(removed.sequences[0]!.tracks[0]!.clips[1]!)).toBeNull()
  })

  it('sets, updates and removes clip opacity', () => {
    const document = fixture()
    const sequenceId = document.sequences[0]!.id
    const dimmed = applyTimelineCommand(document, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: 0.4,
    })
    expect(dimmed.sequences[0]!.tracks[0]!.clips.find(c => c.id === 'clip-8')!.opacity).toBe(0.4)
    const changed = applyTimelineCommand(dimmed, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: 0.75,
    })
    expect(changed.sequences[0]!.tracks[0]!.clips.find(c => c.id === 'clip-8')!.opacity).toBe(0.75)
    const cleared = applyTimelineCommand(changed, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: null,
    })
    expect(cleared.sequences[0]!.tracks[0]!.clips.find(c => c.id === 'clip-8')!.opacity).toBeUndefined()
    expect(() => applyTimelineCommand(document, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: 1.5,
    })).toThrow('invalid clip opacity')
    expect(() => applyTimelineCommand(document, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: Number.NaN,
    })).toThrow('invalid clip opacity')
    const locked = applyTimelineCommand(document, {
      kind: 'set_track_state', sequenceId, trackId: 'track-0', patch: { locked: true },
    })
    expect(() => applyTimelineCommand(locked, {
      kind: 'set_clip_opacity', sequenceId, trackId: 'track-0', clipId: 'clip-8', opacity: 0.5,
    })).toThrow('locked')
  })

  it('adds, updates and removes sequence markers', () => {
    const document = fixture()
    const sequenceId = document.sequences[0]!.id
    const withMarker = applyTimelineCommand(document, {
      kind: 'add_marker', sequenceId, marker: { id: 'marker-1', timelineTick: 1_000_000, color: '#ff8800' },
    })
    expect(withMarker.sequences[0]!.markers).toEqual([{ id: 'marker-1', timelineTick: 1_000_000, color: '#ff8800' }])
    expect(() => applyTimelineCommand(withMarker, {
      kind: 'add_marker', sequenceId, marker: { id: 'marker-1', timelineTick: 2_000_000 },
    })).toThrow('duplicate marker')
    const moved = applyTimelineCommand(withMarker, {
      kind: 'update_marker', sequenceId, markerId: 'marker-1',
      patch: { timelineTick: 2_500_000, label: 'Хук', color: '#00ff00' },
    })
    expect(moved.sequences[0]!.markers).toEqual([
      { id: 'marker-1', timelineTick: 2_500_000, color: '#00ff00', label: 'Хук' },
    ])
    const cleared = applyTimelineCommand(moved, {
      kind: 'update_marker', sequenceId, markerId: 'marker-1', patch: { color: null, label: null },
    })
    expect(cleared.sequences[0]!.markers).toEqual([{ id: 'marker-1', timelineTick: 2_500_000 }])
    expect(() => applyTimelineCommand(moved, {
      kind: 'update_marker', sequenceId, markerId: 'missing', patch: { timelineTick: 1 },
    })).toThrow('missing marker')
    expect(() => applyTimelineCommand(moved, {
      kind: 'update_marker', sequenceId, markerId: 'marker-1', patch: { timelineTick: -1 },
    })).toThrow('invalid marker tick')
    expect(() => applyTimelineCommand(moved, {
      kind: 'update_marker', sequenceId, markerId: 'marker-1', patch: { color: 'red' },
    })).toThrow('invalid marker color')
    const removed = applyTimelineCommand(withMarker, { kind: 'remove_marker', sequenceId, markerId: 'marker-1' })
    expect(removed.sequences[0]!.markers).toEqual([])
    expect(() => applyTimelineCommand(removed, { kind: 'remove_marker', sequenceId, markerId: 'marker-1' })).toThrow('missing marker')
  })

  it('lists track gaps and closes them by shifting downstream clips left', () => {
    const document = fixture()
    const sequenceId = document.sequences[0]!.id
    expect(projectTrackGaps(document.sequences[0]!.tracks[0]!)).toEqual([
      { startTick: 3_000_000, endTick: 4_000_000 },
      { startTick: 7_000_000, endTick: 8_000_000 },
    ])
    expect(projectTrackGaps(document.sequences[0]!.tracks[1]!)![0]).toEqual({ startTick: 0, endTick: 500_000 })
    const closed = applyTimelineCommand(document, {
      kind: 'close_track_gap', sequenceId, trackId: 'track-0', gapStartTick: 3_000_000,
    })
    const starts = closed.sequences[0]!.tracks[0]!.clips
      .map((clip) => [clip.id, clip.timelineStartTick] as const)
    expect(starts).toEqual([['clip-0', 0], ['clip-8', 3_000_000], ['clip-16', 7_000_000]])
    expect(projectTrackGaps(closed.sequences[0]!.tracks[0]!)).toEqual([
      { startTick: 6_000_000, endTick: 7_000_000 },
    ])
    expect(() => applyTimelineCommand(document, {
      kind: 'close_track_gap', sequenceId, trackId: 'track-0', gapStartTick: 4_000_000,
    })).toThrow('no gap')
    const locked = applyTimelineCommand(document, {
      kind: 'set_track_state', sequenceId, trackId: 'track-0', patch: { locked: true },
    })
    expect(() => applyTimelineCommand(locked, {
      kind: 'close_track_gap', sequenceId, trackId: 'track-0', gapStartTick: 3_000_000,
    })).toThrow('locked')
  })

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

describe('split and ripple delete', () => {
  function singleClipDocument(): ProjectDocument {
    const document = fixture()
    const track = document.sequences[0]!.tracks[0]!
    track.clips = [{
      id: 'clip-main',
      mediaId: 'media-video-0',
      timelineStartTick: 1_000_000,
      durationTicks: 10_000_000,
      sourceInTick: 2_000_000,
      sourceOutTick: 12_000_000,
      effects: [
        { id: 'effect-a', kind: 'legacy_edit', enabled: true, parameters: { speed: 1 } },
        { id: 'effect-b', kind: 'crop', enabled: false, parameters: { x: 1 } },
      ],
    }]
    return document
  }

  it('splits a clip into two source-continuous halves with unique effect ids', () => {
    const document = singleClipDocument()
    const command = splitClipAt(document, 'sequence-main', 'clip-main', 6_000_000)
    const split = applyTimelineCommand(document, command)
    const track = split.sequences[0]!.tracks[0]!
    expect(track.clips).toHaveLength(2)
    const [left, right] = track.clips as [ProjectClip, ProjectClip]
    expect(left.id).toBe('clip-main')
    expect(left.timelineStartTick).toBe(1_000_000)
    expect(left.durationTicks).toBe(5_000_000)
    expect(left.sourceInTick).toBe(2_000_000)
    expect(left.sourceOutTick).toBe(7_000_000)
    expect(right.id).toBe('clip-main-right')
    expect(right.timelineStartTick).toBe(6_000_000)
    expect(right.durationTicks).toBe(5_000_000)
    expect(right.sourceInTick).toBe(7_000_000)
    expect(right.sourceOutTick).toBe(12_000_000)
    const effectIds = track.clips.flatMap((clip) => clip.effects.map((effect) => effect.id))
    expect(new Set(effectIds).size).toBe(effectIds.length)
    expect(right.effects.map((effect) => effect.id)).toEqual(['effect-a-right', 'effect-b-right'])
    expect(left.effects.map((effect) => effect.id)).toEqual(['effect-a', 'effect-b'])
    expect(() => applyTimelineCommand(split, command)).toThrow('duplicate clip id after split')
  })

  it('rejects split points outside the clip and duplicated ids in the payload', () => {
    const document = singleClipDocument()
    expect(() => splitClipAt(document, 'sequence-main', 'clip-main', 1_000_000)).toThrow(
      'split point outside clip',
    )
    expect(() => splitClipAt(document, 'sequence-main', 'clip-main', 11_000_000)).toThrow(
      'split point outside clip',
    )
    const command = splitClipAt(document, 'sequence-main', 'clip-main', 5_000_000)
    const forged: ProjectClip = structuredClone(command.rightClip)
    forged.id = 'clip-1'
    expect(() =>
      applyTimelineCommand(document, { ...command, rightClip: forged }),
    ).toThrow('duplicate clip id after split')
    const broken: ProjectClip = structuredClone(command.rightClip)
    broken.sourceInTick = 99_000_000
    expect(() =>
      applyTimelineCommand(document, { ...command, rightClip: broken }),
    ).toThrow('invalid split geometry')
  })

  it('undo of a split restores the original clip through structural history', () => {
    const document = singleClipDocument()
    const history = new StructuralHistory()
    const split = history.execute(
      document,
      splitClipAt(document, 'sequence-main', 'clip-main', 6_000_000),
    )
    expect(split.sequences[0]!.tracks[0]!.clips).toHaveLength(2)
    const restored = history.undo(split)
    expect(restored.sequences[0]!.tracks[0]!.clips).toHaveLength(1)
    expect(restored.sequences[0]!.tracks[0]!.clips[0]!.id).toBe('clip-main')
    expect(restored.sequences[0]!.tracks[0]!.clips[0]!.durationTicks).toBe(10_000_000)
  })

  it('ripple deletes a clip and shifts downstream clips left by its duration', () => {
    const document = singleClipDocument()
    const track = document.sequences[0]!.tracks[0]!
    track.clips = [
      ...track.clips,
      { id: 'clip-b', mediaId: 'media-video-0', timelineStartTick: 11_000_000, durationTicks: 4_000_000, sourceInTick: 0, sourceOutTick: 4_000_000, effects: [] },
      { id: 'clip-c', mediaId: 'media-video-0', timelineStartTick: 17_000_000, durationTicks: 4_000_000, sourceInTick: 0, sourceOutTick: 4_000_000, effects: [] },
    ]
    const ripple = rippleDeleteClip(document, 'sequence-main', track.id, 'clip-main')
    const after = applyTimelineCommand(document, ripple)
    const clips = after.sequences[0]!.tracks[0]!.clips
    expect(clips.map((clip) => clip.id)).toEqual(['clip-b', 'clip-c'])
    expect(clips[0]!.timelineStartTick).toBe(1_000_000)
    expect(clips[1]!.timelineStartTick).toBe(7_000_000)
    const history = new StructuralHistory()
    const executed = history.execute(document, ripple)
    expect(history.undo(executed).sequences[0]!.tracks[0]!.clips.map((clip) => clip.id)).toEqual([
      'clip-main',
      'clip-b',
      'clip-c',
    ])
  })

  it('ripple delete keeps clips that start before the removed clip untouched', () => {
    const document = singleClipDocument()
    const track = document.sequences[0]!.tracks[0]!
    track.clips = [
      { id: 'clip-early', mediaId: 'media-video-0', timelineStartTick: 0, durationTicks: 500_000, sourceInTick: 0, sourceOutTick: 500_000, effects: [] },
      ...track.clips,
    ]
    const after = applyTimelineCommand(
      document,
      rippleDeleteClip(document, 'sequence-main', track.id, 'clip-main'),
    )
    const clips = after.sequences[0]!.tracks[0]!.clips
    expect(clips.map((clip) => clip.id)).toEqual(['clip-early'])
    expect(clips[0]!.timelineStartTick).toBe(0)
  })
})
