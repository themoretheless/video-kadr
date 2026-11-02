import { updateCompositionAudioStream, detachCompositionVideoAudio, reattachCompositionVideoAudio, updateCompositionClipGain } from './composition.svelte.js'
import { nestedComposition, closeNestedComposition } from './nestedComposition.svelte.js'
import { buildCompositionRenderRequest } from '../composition/payload'
import { importPreferences } from '$lib/state/importPreferences.svelte.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api'
import { COMPOSITION_TIME_BASE, type Composition, type VideoTrack } from '../composition/types'
import {
  addMediaInfoToComposition,
  addLibraryEntryToComposition,
  applyCompositionTextStyleToTrack,
  addVoiceoverMediaInfoToComposition,
  addCompositionTrack,
  insertCompositionPlacements,
  addCompositionMarkerAtPlayhead,
  addCompositionVideoMask,
  applyCompositionTrackedAnimation,
  compositionMulticamGroups,
  compositionRenderOutput,
  compositionSourceFromLibraryEntry,
  compositionMarkerList,
  compositionState,
  commitDocument,
  setCompositionClipLocked,
  createCompositionMulticamGroup,
  deleteCompositionAutomationKeyframe,
  estimateCompositionMulticamSync,
  exportComposition,
  exportSelectedTextTrackSrt,
  flushCompositionAutosaveForTests,
  freezeCompositionClipAtPlayhead,
  getCompositionExportUnavailableReason,
  importSrtToComposition,
  newComposition,
  openCompositionProject,
  refreshOpenCompositionProject,
  refreshCompositionProjects,
  reloadCompositionDraftForTests,
  resetCompositionForTests,
  saveCompositionProject,
  magnetizeCompositionTrack,
  moveCompositionClip,
  rebuildCompositionMulticamGroup,
  redoComposition,
  recordCompositionMulticamSwitch,
  relinkCompositionSource,
  replaceCompositionAutoBeatMarkers,
  replaceCompositionDocument,
  removeSilenceFromSelectedCompositionClip,
  removeCompositionMarker,
  rippleDeleteSelectedCompositionClip,
  seekCompositionMarker,
  selectCompositionClip,
  setCompositionStorageForTests,
  setCompositionAutomationInterpolation,
  setCompositionAutomationKeyframe,
  setCompositionPlayhead,
  setCompositionExportRangePoint,
  setCompositionProjectName,
  splitSelectedCompositionClip,
  syncCompositionLibrary,
  setCompositionVisualInterpolation,
  setCompositionVisualKeyframe,
  toggleCompositionTrackFlag,
  undoComposition,
  updateCompositionVideoMask,
  updateCompositionAutomationKeyframe,
  updateCompositionVisualKeyframe,
  updateCompositionClipSpeed,
  updateCompositionFrameInterpolation,
  updateCompositionMarker,
  updateCompositionPlaybackMode,
  updateCompositionSpeedRamp,
  updateCompositionExportSettings,
  updateCompositionStabilization,
  updateCompositionTextStyle,
  updateCompositionVideoAudio,
  updateCompositionAudioMix,
  updateCompositionSourceGainKeys,
} from './composition.svelte.js'
import type { Capabilities, Job, MediaEntry, MediaInfo } from '../types'
import { setAuthSessionForTests } from './auth.svelte.js'

vi.mock('../api', () => ({
  renderComposition: vi.fn(),
  pollJob: vi.fn(),
  cancelJob: vi.fn(),
  getCompositionProjects: vi.fn(() => Promise.resolve([])),
  getCompositionProject: vi.fn(),
  getCompositionProjectIfChanged: vi.fn(),
  getCompositionProjectsIfChanged: vi.fn(),
  createCompositionProject: vi.fn(),
  updateCompositionProject: vi.fn(),
  updateNestedCompositionProject: vi.fn(),
  createNestedCompositionProject: vi.fn(),
  deleteCompositionProject: vi.fn(),
}))

const capabilities: Capabilities = {
  schemaVersion: 1,
  toolFingerprint: 'test',
  formats: [],
  codecs: [],
  filters: [],
  hardware: [],
  features: [{ id: 'composition-v1', label: 'Composition v1', available: true }],
}

const video: MediaInfo = {
  id: 'source-video',
  url: '/files/sources/video.mp4',
  filename: 'video.mp4',
  mediaType: 'video',
  duration: 10,
  width: 1920,
  height: 1080,
  fps: 30,
  acodec: 'aac',
}

beforeEach(() => {
  closeNestedComposition()
  // Exercise embedded-audio authoring independently of the default import policy.
  importPreferences.autoPairAudio = false
  setAuthSessionForTests({
    user: { id: 'user-1', username: 'alice', createdAt: 1 },
    token: 'session-token',
    expiresAt: 9999999999,
  })
  vi.clearAllMocks()
  setCompositionStorageForTests(null)
  resetCompositionForTests()
})

afterEach(() => {
  setAuthSessionForTests(null)
  setCompositionStorageForTests(undefined)
})

const DRAFTS_KEY = 'video-kadr:composition-drafts:v2'
const LEGACY_DRAFT_KEY = 'video-kadr:composition-draft:v1'

function memoryStorage(values: Map<string, string>) {
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
  }
}

function activeStoredDraft<T = { document: Composition; exportSettings?: unknown }>(values: Map<string, string>): T {
  const drafts = JSON.parse(values.get(DRAFTS_KEY)!)
  return drafts.activeProjectId === null
    ? drafts.unsaved
    : drafts.projects[drafts.activeProjectId]
}

describe('composition editor state', () => {
  it('uses the enabled import preference for ordinary media and library entrypoints', () => {
    importPreferences.autoPairAudio = true
    addMediaInfoToComposition(video)
    expect(compositionState.document.links).toHaveLength(1)
    resetCompositionForTests()
    addLibraryEntryToComposition({ ...video, kind: 'source', createdAt: 1 } as MediaEntry)
    expect(compositionState.document.links).toHaveLength(1)
    resetCompositionForTests()
    addMediaInfoToComposition({ ...video, acodec: null })
    expect(compositionState.document.links ?? []).toHaveLength(0)
    expect(compositionState.document.tracks.filter((track) => track.kind === 'audio')).toHaveLength(0)
  })

  it('imports a linked video/audio pair atomically with one audible audio owner and one Undo', () => {
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const videoId = addMediaInfoToComposition(video, { pairAudio: true })
    const videoClip = compositionState.document.tracks.flatMap((track) => track.clips.map((clip) => clip)).find((clip) => clip.id === videoId)!
    const audioTrack = compositionState.document.tracks.find((track) => track.kind === 'audio')!
    const audio = audioTrack.clips[0]!
    expect(videoClip).toMatchObject({ kind: 'video', sourceAudioEnabled: false })
    expect(audio).toMatchObject({ kind: 'audio', sourceId: video.id, timelineStartTicks: videoClip.timelineStartTicks, sourceInTicks: 0, sourceOutTicks: video.duration * COMPOSITION_TIME_BASE, gain: 1, pan: 0 })
    expect(compositionState.document.links?.[0]?.memberIds).toEqual([videoId, audio.id])
    expect(compositionState.ui.selectedClipIds).toEqual([videoId, audio.id])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.media[video.id]).toBeUndefined()
  })

  it('rejects rebinding a protected source without changing document, media or history', () => {
    const clipId = addMediaInfoToComposition(video)
    setCompositionClipLocked(clipId, true)
    const before = JSON.stringify({ document: compositionState.document, media: compositionState.media, history: compositionState.history })
    const replacement = { ...compositionState.media, [video.id]: { ...compositionState.media[video.id]!, url: '/files/sources/replaced.mp4' } }
    expect(() => commitDocument(compositionState.document, replacement)).toThrow('media binding')
    expect(JSON.stringify({ document: compositionState.document, media: compositionState.media, history: compositionState.history })).toBe(before)
  })

  it('registers video/audio/image sources on compatible tracks without losing authoring data', () => {
    addMediaInfoToComposition(video)
    setCompositionPlayhead(5 * COMPOSITION_TIME_BASE)
    addMediaInfoToComposition({
      id: 'source-audio',
      url: '/files/sources/music.wav',
      filename: 'music.wav',
      mediaType: 'audio',
      duration: 20,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    addMediaInfoToComposition({
      id: 'source-image',
      url: '/files/sources/card.webp',
      filename: 'card.webp',
      mediaType: 'image',
      duration: 0,
      width: 800,
      height: 600,
    })

    expect(Object.keys(compositionState.document.sources)).toEqual([
      'source-video',
      'source-audio',
      'source-image',
    ])
    expect(compositionState.document.tracks.map((track) => track.kind)).toEqual(['image', 'video', 'audio'])
    const audio = compositionState.document.tracks.find((track) => track.kind === 'audio')!
    expect(audio.clips[0]).toMatchObject({ timelineStartTicks: 5 * COMPOSITION_TIME_BASE, sourceOutTicks: 5 * COMPOSITION_TIME_BASE })
    expect(getCompositionExportUnavailableReason(capabilities)).toBeNull()

    const imageTrack = compositionState.document.tracks.find((track) => track.kind === 'image')!
    toggleCompositionTrackFlag(imageTrack.id, 'hidden')
    expect(getCompositionExportUnavailableReason(capabilities)).toBeNull()
  })

  it('adds a validated voiceover atomically to an audio track at the playhead', () => {
    addMediaInfoToComposition(video)
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)

    const clipId = addVoiceoverMediaInfoToComposition({
      id: 'voiceover-source',
      url: '/files/sources/voiceover.webm',
      filename: 'voiceover.webm',
      mediaType: 'audio',
      duration: 4,
      width: 0,
      height: 0,
      acodec: 'opus',
    })

    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'audio')!
    expect(track.clips[0]).toMatchObject({
      id: clipId,
      kind: 'audio',
      sourceId: 'voiceover-source',
      timelineStartTicks: 3 * COMPOSITION_TIME_BASE,
      sourceOutTicks: 4 * COMPOSITION_TIME_BASE,
    })
    expect(compositionState.ui).toMatchObject({ selectedTrackId: track.id, selectedClipId: clipId })
    expect(compositionState.media['voiceover-source']).toMatchObject({ mediaType: 'audio', acodec: 'opus' })

    expect(() => addVoiceoverMediaInfoToComposition({
      id: 'not-voiceover',
      url: '/files/sources/not-voiceover.webm',
      filename: 'not-voiceover.webm',
      mediaType: 'video',
      duration: 1,
      width: 1280,
      height: 720,
    })).toThrow('не распознана как аудио')
    expect(compositionState.document.sources).not.toHaveProperty('not-voiceover')
    expect(compositionState.media).not.toHaveProperty('not-voiceover')
  })

  it('prunes stale media bindings only for an authoritative successful library snapshot', () => {
    addMediaInfoToComposition(video)
    const sourceBefore = JSON.parse(JSON.stringify(compositionState.document.sources[video.id]!)) as unknown
    const tracksBefore = JSON.parse(JSON.stringify(compositionState.document.tracks)) as unknown

    syncCompositionLibrary([], false)
    expect(compositionState.media).toHaveProperty(video.id)

    syncCompositionLibrary([], true)
    expect(compositionState.media).not.toHaveProperty(video.id)
    expect(compositionState.document.sources[video.id]).toEqual(sourceBefore)
    expect(compositionState.document.tracks).toEqual(tracksBefore)
  })

  it('keeps split edits in bounded undo/redo history', () => {
    const originalId = addMediaInfoToComposition(video)
    const originalDocument = JSON.parse(JSON.stringify(compositionState.document)) as unknown
    setCompositionPlayhead(4 * COMPOSITION_TIME_BASE)
    const rightId = splitSelectedCompositionClip()

    expect(rightId).toBeTruthy()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(compositionState.document.tracks[0]!.clips[0]!.id).toBe(originalId)
    undoComposition()
    expect(compositionState.document).toEqual(originalDocument)
    redoComposition()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
  })

  it('removes selected clip silence as one undoable multitrack edit', () => {
    const clipId = addMediaInfoToComposition(video)
    const original = JSON.parse(JSON.stringify(compositionState.document)) as Composition

    expect(removeSilenceFromSelectedCompositionClip([
      { start: 0, end: 2 },
      { start: 6, end: 10 },
    ])).toBe(2)
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ id: clipId, sourceInTicks: 0, sourceOutTicks: 2 * COMPOSITION_TIME_BASE })
    expect(compositionState.document.tracks[0]!.clips[1]).toMatchObject({ timelineStartTicks: 2 * COMPOSITION_TIME_BASE, sourceInTicks: 6 * COMPOSITION_TIME_BASE })
    expect(compositionState.ui.selectedClipId).toBe(clipId)

    undoComposition()
    expect(compositionState.document).toEqual(original)
  })

  it('keeps marker edits in document history and supports exact seek/delete', () => {
    addMediaInfoToComposition(video)
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    const markerId = addCompositionMarkerAtPlayhead('Beat')
    updateCompositionMarker(markerId, { tick: 3 * COMPOSITION_TIME_BASE, label: 'Drop', color: '#38bdf8' })
    expect(compositionMarkerList()).toEqual([
      { id: markerId, tick: 3 * COMPOSITION_TIME_BASE, label: 'Drop', color: '#38bdf8' },
    ])
    undoComposition()
    expect(compositionMarkerList()[0]).toMatchObject({ tick: 2 * COMPOSITION_TIME_BASE, label: 'Beat' })
    redoComposition()
    setCompositionPlayhead(0)
    seekCompositionMarker(markerId)
    expect(compositionState.transport.playheadTicks).toBe(3 * COMPOSITION_TIME_BASE)
    removeCompositionMarker(markerId)
    expect(compositionMarkerList()).toEqual([])
    undoComposition()
    expect(compositionMarkerList()).toHaveLength(1)
  })

  it('replaces Auto Beat markers atomically with stable ids while preserving manual markers', () => {
    const manualId = addCompositionMarkerAtPlayhead('Manual cue')
    const beforeDepth = compositionState.history.past.length
    const values = new Map<string, string>()
    setCompositionStorageForTests({
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value) },
    })

    expect(replaceCompositionAutoBeatMarkers([
      { tick: 2_000_000, strength: 1.5 },
      { tick: 1_000_000, strength: 2 },
      { tick: 1_000_000, strength: 1 },
    ], 120)).toBe(2)
    expect(compositionState.history.past).toHaveLength(beforeDepth + 1)
    const first = compositionMarkerList()
    expect(first).toEqual([
      expect.objectContaining({ id: manualId, tick: 0, label: 'Manual cue' }),
      {
        id: 'auto-beat-1000000',
        tick: 1_000_000,
        label: 'Auto Beat 1 · ≈ 120 BPM',
        color: '#f97316',
        origin: 'auto_beat',
      },
      {
        id: 'auto-beat-2000000',
        tick: 2_000_000,
        label: 'Auto Beat 2 · ≈ 120 BPM',
        color: '#f97316',
        origin: 'auto_beat',
      },
    ])
    const firstDepth = compositionState.history.past.length
    replaceCompositionAutoBeatMarkers([
      { tick: 1_000_000, strength: 2 },
      { tick: 2_000_000, strength: 1.5 },
    ], 120)
    expect(compositionState.history.past).toHaveLength(firstDepth)
    expect(compositionMarkerList()).toEqual(first)
    flushCompositionAutosaveForTests()
    expect(activeStoredDraft<{ document: { markers?: unknown } }>(values).document.markers).toEqual(first)

    replaceCompositionAutoBeatMarkers([{ tick: 3_000_000, strength: 3 }], 98.5)
    expect(compositionMarkerList()).toEqual([
      expect.objectContaining({ id: manualId, label: 'Manual cue' }),
      expect.objectContaining({
        id: 'auto-beat-3000000',
        tick: 3_000_000,
        label: 'Auto Beat 1 · ≈ 98.5 BPM',
        origin: 'auto_beat',
      }),
    ])
    undoComposition()
    expect(compositionMarkerList()).toEqual(first)
    redoComposition()
    expect(compositionMarkerList().map((marker) => marker.id)).toEqual([manualId, 'auto-beat-3000000'])
  })

  it('rolls back an Auto Beat replacement that would exceed marker capacity', () => {
    replaceCompositionDocument({
      ...compositionState.document,
      markers: Array.from({ length: 255 }, (_, index) => ({
        id: `manual-${index}`,
        tick: index,
        label: `Manual ${index}`,
      })),
    } as Composition)
    const before = JSON.stringify(compositionState.document)
    const historyDepth = compositionState.history.past.length
    expect(() => replaceCompositionAutoBeatMarkers([
      { tick: 1_000_000, strength: 2 },
      { tick: 2_000_000, strength: 2 },
    ], 120)).toThrow('256')
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(historyDepth)
  })

  it('runs selected-track ripple delete and one-shot magnet through history with locked-track errors', () => {
    const leftId = addMediaInfoToComposition(video)
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    const rightId = splitSelectedCompositionClip()!
    const track = compositionState.document.tracks[0]!
    moveCompositionClip(rightId, track.id, 5 * COMPOSITION_TIME_BASE, false)
    magnetizeCompositionTrack(track.id)
    expect(compositionState.document.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([0, 2 * COMPOSITION_TIME_BASE])

    selectCompositionClip(track.id, leftId)
    rippleDeleteSelectedCompositionClip()
    expect(compositionState.document.tracks[0]!.clips).toEqual([
      expect.objectContaining({ id: rightId, timelineStartTicks: 0 }),
    ])
    undoComposition()
    toggleCompositionTrackFlag(track.id, 'locked')
    expect(() => magnetizeCompositionTrack(track.id)).toThrow('locked')
  })

  it('authors visual keyframes and masks immutably through undo/redo history', () => {
    addMediaInfoToComposition(video)
    addCompositionTrack('video')
    const overlayId = addMediaInfoToComposition({
      ...video,
      id: 'source-overlay',
      filename: 'overlay.mp4',
      url: '/files/sources/overlay.mp4',
      duration: 4,
      width: 640,
      height: 360,
      acodec: null,
    })
    setCompositionPlayhead(COMPOSITION_TIME_BASE)
    setCompositionVisualKeyframe(overlayId, 'x', 20)
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    setCompositionVisualKeyframe(overlayId, 'x', 120)
    setCompositionVisualInterpolation(overlayId, 'x', 'ease_in_out')
    updateCompositionVisualKeyframe(overlayId, 'x', COMPOSITION_TIME_BASE, 500_000, 30)
    const maskId = addCompositionVideoMask(overlayId, 'ellipse')
    updateCompositionVideoMask(overlayId, maskId, { x: 0.6, feather: 0.25, inverted: true })

    const overlay = compositionState.document.tracks
      .find((track): track is VideoTrack => track.kind === 'video' && track.id === compositionState.ui.selectedTrackId)
    const authored = overlay?.clips.find((clip) => clip.id === overlayId)
    expect(authored?.animation?.x).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'ease_in_out',
        keyframes: [{ tick: 500_000, value: 30 }, { tick: 2 * COMPOSITION_TIME_BASE, value: 120 }],
      },
    })
    expect(authored?.masks?.[0]).toMatchObject({
      id: maskId,
      shape: 'ellipse',
      x: { mode: 'constant', value: 0.6 },
      feather: 0.25,
      inverted: true,
    })

    undoComposition()
    const afterUndo = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(afterUndo?.masks?.[0]).toMatchObject({ feather: 0, inverted: false })
    redoComposition()
    const afterRedo = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(afterRedo?.masks?.[0]).toMatchObject({ feather: 0.25, inverted: true })
  })

  it('authors primary/audio gain-pan and mask geometry keyframes as atomic autosaved edits', () => {
    const primaryId = addMediaInfoToComposition(video)
    const values = new Map<string, string>()
    setCompositionStorageForTests({
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value) },
    })

    setCompositionPlayhead(COMPOSITION_TIME_BASE)
    const historyBefore = compositionState.history.past.length
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: primaryId }, 'gain', 0.5)
    expect(compositionState.history.past).toHaveLength(historyBefore + 1)
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: primaryId }, 'gain', 1.5)
    setCompositionAutomationInterpolation({ kind: 'audio', clipId: primaryId }, 'gain', 'ease_in')
    updateCompositionAutomationKeyframe({ kind: 'audio', clipId: primaryId }, 'gain', COMPOSITION_TIME_BASE, 500_000, 0.4)

    setCompositionPlayhead(0)
    const audioId = addMediaInfoToComposition({
      id: 'automated-audio-source',
      url: '/files/sources/automated.wav',
      filename: 'automated.wav',
      mediaType: 'audio',
      duration: 5,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: audioId }, 'pan', -1)
    setCompositionPlayhead(4 * COMPOSITION_TIME_BASE)
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: audioId }, 'pan', 1)
    setCompositionAutomationInterpolation({ kind: 'audio', clipId: audioId }, 'pan', 'ease_out')

    addCompositionTrack('video')
    setCompositionPlayhead(COMPOSITION_TIME_BASE)
    const overlayId = addMediaInfoToComposition({
      ...video,
      id: 'automated-mask-source',
      url: '/files/sources/automated-mask.mp4',
      filename: 'automated-mask.mp4',
      duration: 4,
      width: 640,
      height: 360,
      acodec: null,
    })
    const maskId = addCompositionVideoMask(overlayId, 'rectangle')
    const unrelatedBefore = compositionState.document.tracks
      .filter((track) => !track.clips.some((clip) => clip.id === overlayId))
      .map((track) => JSON.stringify(track))
    const maskTarget = { kind: 'mask', clipId: overlayId, maskId } as const
    setCompositionAutomationKeyframe(maskTarget, 'x', 0.25)
    setCompositionAutomationKeyframe(maskTarget, 'rotationDegrees', -30)
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    setCompositionAutomationKeyframe(maskTarget, 'x', 0.75)
    setCompositionAutomationKeyframe(maskTarget, 'rotationDegrees', 30)
    setCompositionAutomationInterpolation(maskTarget, 'x', 'ease_in_out')
    const originalMaskTick = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)!.masks![0]!.x
    if (originalMaskTick.mode !== 'keyframes') throw new Error('Expected authored mask keyframes')
    updateCompositionAutomationKeyframe(maskTarget, 'x', originalMaskTick.track.keyframes[0]!.tick, 250_000, 0.3)
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    updateCompositionVideoMask(overlayId, maskId, { x: 0.6 })

    const primary = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === primaryId)
    const audio = compositionState.document.tracks
      .flatMap((track) => track.kind === 'audio' ? track.clips : [])
      .find((clip) => clip.id === audioId)
    const overlay = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(primary?.audioAnimation?.gain).toMatchObject({
      track: { interpolation: 'ease_in', keyframes: [{ tick: 500_000, value: 0.4 }, { tick: 3_000_000, value: 1.5 }] },
    })
    expect(audio?.audioAnimation?.pan).toMatchObject({
      track: { interpolation: 'ease_out', keyframes: [{ tick: 0, value: -1 }, { tick: 4_000_000, value: 1 }] },
    })
    expect(overlay?.masks?.[0]?.x).toMatchObject({
      track: {
        interpolation: 'ease_in_out',
        keyframes: [
          { tick: 250_000, value: 0.3 },
          { tick: 2_000_000, value: 0.6 },
          { tick: 3_000_000, value: 0.75 },
        ],
      },
    })
    expect(overlay?.masks?.[0]?.rotationDegrees).toMatchObject({
      track: { keyframes: [{ tick: 1_000_000, value: -30 }, { tick: 3_000_000, value: 30 }] },
    })
    expect(compositionState.document.tracks
      .filter((track) => !track.clips.some((clip) => clip.id === overlayId))
      .map((track) => JSON.stringify(track))).toEqual(unrelatedBefore)

    deleteCompositionAutomationKeyframe(maskTarget, 'x', 250_000)
    undoComposition()
    expect(compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)?.masks?.[0]?.x).toMatchObject({
        track: { keyframes: [{ tick: 250_000 }, { tick: 2_000_000 }, { tick: 3_000_000 }] },
      })
    redoComposition()
    deleteCompositionAutomationKeyframe({ kind: 'audio', clipId: audioId }, 'pan', 0)
    flushCompositionAutosaveForTests()
    const saved = activeStoredDraft(values) as { document: Composition }
    expect(saved.document.tracks
      .flatMap((track) => track.kind === 'audio' ? track.clips : [])
      .find((clip) => clip.id === audioId)?.audioAnimation?.pan).toMatchObject({
        track: { keyframes: [{ tick: 4_000_000, value: 1 }] },
      })
  })

  it('applies paired tracked X/Y keyframes as one guarded autosaved history edit', () => {
    const primaryId = addMediaInfoToComposition(video)
    addCompositionTrack('video')
    const overlayId = addMediaInfoToComposition({
      ...video,
      id: 'source-tracked-overlay',
      filename: 'tracked-overlay.mp4',
      url: '/files/sources/tracked-overlay.mp4',
      duration: 4,
      width: 640,
      height: 360,
      acodec: null,
    })
    setCompositionVisualKeyframe(overlayId, 'opacity', 0.75, 0)

    const before = JSON.stringify(compositionState.document)
    const historyDepth = compositionState.history.past.length
    const values = new Map<string, string>()
    setCompositionStorageForTests({
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value) },
    })
    const trackedAnimation = {
      opacity: { mode: 'constant', value: 0.1 },
      x: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'linear',
          keyframes: [{ tick: 0, value: 12 }, { tick: 500_000, value: 30 }, { tick: 1_000_000, value: 48 }],
        },
      },
      y: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'linear',
          keyframes: [{ tick: 0, value: -4 }, { tick: 500_000, value: 8 }, { tick: 1_000_000, value: 20 }],
        },
      },
    } as const

    applyCompositionTrackedAnimation(overlayId, trackedAnimation)
    expect(compositionState.history.past).toHaveLength(historyDepth + 1)
    let tracked = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(tracked?.animation).toEqual({
      opacity: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'linear',
          keyframes: [{ tick: 0, value: 0.75 }],
        },
      },
      x: trackedAnimation.x,
      y: trackedAnimation.y,
    })

    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    redoComposition()
    tracked = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(tracked?.animation?.x).toEqual(trackedAnimation.x)
    expect(tracked?.animation?.y).toEqual(trackedAnimation.y)
    flushCompositionAutosaveForTests()
    const saved = activeStoredDraft<{
      document: { tracks: Array<{ kind: string; clips: Array<{ id: string; animation: typeof trackedAnimation }> }> }
    }>(values)
    const savedTracked = saved.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === overlayId)
    expect(savedTracked?.animation.x).toEqual(trackedAnimation.x)
    expect(savedTracked?.animation.y).toEqual(trackedAnimation.y)

    const overlayTrack = compositionState.document.tracks.find((track) => track.clips.some((clip) => clip.id === overlayId))!
    toggleCompositionTrackFlag(overlayTrack.id, 'locked')
    const lockedDocument = JSON.stringify(compositionState.document)
    const lockedHistoryDepth = compositionState.history.past.length
    expect(() => applyCompositionTrackedAnimation(overlayId, trackedAnimation)).toThrow('заблокирована')
    expect(JSON.stringify(compositionState.document)).toBe(lockedDocument)
    expect(compositionState.history.past).toHaveLength(lockedHistoryDepth)
    applyCompositionTrackedAnimation(primaryId, trackedAnimation)
    const primary = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === primaryId)
    expect(primary?.animation?.x).toEqual(trackedAnimation.x)
    expect(primary?.animation?.y).toEqual(trackedAnimation.y)
  })

  it('gates export on capability and polls a supported render job', async () => {
    addMediaInfoToComposition(video)
    expect(getCompositionExportUnavailableReason({ ...capabilities, features: undefined })).toContain('не объявил')
    expect(
      getCompositionExportUnavailableReason({
        ...capabilities,
        features: [{ id: 'composition-v1', label: 'Composition v1', available: false, reason: 'ffmpeg missing' }],
      }),
    ).toBe('ffmpeg missing')

    vi.mocked(api.renderComposition).mockResolvedValue({ jobId: 'job-1' })
    vi.mocked(api.pollJob).mockImplementation(async (_id, onTick) => {
      onTick?.({ id: 'job-1', status: 'running', progress: 42, stage: 'processing' })
      return {
        id: 'job-1',
        status: 'done',
        result: { id: 'out-1', url: '/files/outputs/out.mp4', filename: 'out.mp4' },
      } as Job
    })

    await exportComposition(capabilities)

    expect(api.renderComposition).toHaveBeenCalledOnce()
    const request = vi.mocked(api.renderComposition).mock.calls[0]![0]
    expect(request.composition.sources['source-video']).not.toHaveProperty('url')
    expect(request.output).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
    })
    expect(compositionState.export.result).toMatchObject({ filename: 'out.mp4' })
    expect(compositionState.export.running).toBe(false)
  })

  it('sends an exact In/Out range without mutating the composition', async () => {
    addMediaInfoToComposition(video)
    const documentBefore = JSON.stringify(compositionState.document)
    setCompositionPlayhead(250_000)
    setCompositionExportRangePoint('in')
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('обе границы')
    setCompositionPlayhead(1_250_000)
    setCompositionExportRangePoint('out')
    expect(getCompositionExportUnavailableReason(capabilities)).toBeNull()

    vi.mocked(api.renderComposition).mockResolvedValue({ jobId: 'range-job' })
    vi.mocked(api.pollJob).mockResolvedValue({
      id: 'range-job',
      status: 'done',
      result: { id: 'range-out', url: '/files/outputs/range.mp4', filename: 'range.mp4' },
    } as Job)
    await exportComposition(capabilities)

    expect(vi.mocked(api.renderComposition).mock.calls.at(-1)?.[0].output).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      range: { startTicks: 250_000, endTicks: 1_250_000 },
    })
    expect(JSON.stringify(compositionState.document)).toBe(documentBefore)
  })

  it('persists bounded custom bitrate and clears it for incompatible profiles', () => {
    addMediaInfoToComposition(video)
    updateCompositionExportSettings({ videoBitrateKbps: 18_000 })
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      videoBitrateKbps: 18_000,
    })
    undoComposition()
    expect(compositionState.export.videoBitrateKbps).toBeNull()
    redoComposition()
    expect(compositionState.export.videoBitrateKbps).toBe(18_000)
    expect(() => updateCompositionExportSettings({ videoBitrateKbps: 99 })).toThrow('100..200000')

    updateCompositionExportSettings({
      profile: { container: 'mov', profile: 'hq' },
      videoBitrateKbps: undefined,
    })
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mov', profile: 'hq' },
      qualityTier: 'medium',
    })
  })

  it('persists a target file size and keeps it exclusive with custom bitrate', () => {
    addMediaInfoToComposition(video)
    updateCompositionExportSettings({ targetSizeBytes: 25_000_000 })
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      targetSizeBytes: 25_000_000,
    })
    undoComposition()
    expect(compositionState.export.targetSizeBytes).toBeNull()
    redoComposition()
    expect(compositionState.export.targetSizeBytes).toBe(25_000_000)

    expect(() => updateCompositionExportSettings({ videoBitrateKbps: 12_000 })).toThrow('исключают')
    expect(() => updateCompositionExportSettings({ targetSizeBytes: 0 })).toThrow('целым числом')

    updateCompositionExportSettings({
      profile: { container: 'mov', profile: 'hq' },
      targetSizeBytes: undefined,
    })
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mov', profile: 'hq' },
      qualityTier: 'medium',
    })
  })

  it('persists delivery settings through history, autosave, and exact capability gates', () => {
    addMediaInfoToComposition(video)
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
    })

    updateCompositionExportSettings({
      profile: { container: 'webm', codec: 'av1' },
      qualityTier: 'high',
    })
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('composition-webm-av1')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'composition-webm-av1', label: 'WebM AV1', available: false, reason: 'AV1 encoder missing' },
      ],
    })).toBe('AV1 encoder missing')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'composition-webm-av1', label: 'WebM AV1', available: true },
      ],
    })).toBeNull()

    undoComposition()
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
    })
    redoComposition()
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'webm', codec: 'av1' },
      qualityTier: 'high',
    })

    const values = new Map<string, string>()
    setCompositionStorageForTests({
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value) },
    })
    updateCompositionExportSettings({ qualityTier: 'compact' })
    flushCompositionAutosaveForTests()
    expect(activeStoredDraft<{ exportSettings: unknown }>(values).exportSettings).toEqual({
      profile: { container: 'webm', codec: 'av1' },
      qualityTier: 'compact',
    })
  })

  it('gates optical flow capability only for valid active slow-motion requests', () => {
    const clipId = addMediaInfoToComposition(video)
    updateCompositionClipSpeed(clipId, 0.5)
    updateCompositionFrameInterpolation(clipId, 'optical_flow')
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('не объявил поддержку optical-flow')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'optical-flow', label: 'Optical flow', available: false, reason: 'minterpolate missing' },
      ],
    })).toBe('minterpolate missing')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'optical-flow', label: 'Optical flow', available: true },
      ],
    })).toBeNull()

    expect(() => updateCompositionClipSpeed(clipId, 1)).toThrow('Optical flow')
    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'video')!
    expect(() => toggleCompositionTrackFlag(track.id, 'hidden')).toThrow('Optical flow')
    expect(track).toMatchObject({ hidden: false })
  })

  it('authors playback modes through history and gates reverse/freeze capabilities per request', () => {
    const clipId = addMediaInfoToComposition(video)
    updateCompositionPlaybackMode(clipId, { mode: 'reverse' })
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('reverse-playback')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'reverse-playback', label: 'Reverse playback', available: false, reason: 'reverse missing' },
      ],
    })).toBe('reverse missing')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'reverse-playback', label: 'Reverse playback', available: true },
      ],
    })).toBeNull()

    updateCompositionPlaybackMode(clipId, { mode: 'forward' })
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    expect(freezeCompositionClipAtPlayhead(clipId)).toBe(3 * COMPOSITION_TIME_BASE)
    const frozen = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === clipId)
    expect(frozen).toMatchObject({
      playbackMode: { mode: 'freeze', sourceTick: 3 * COMPOSITION_TIME_BASE },
      sourceAudioEnabled: false,
    })
    expect(() => updateCompositionVideoAudio(clipId, { sourceAudioEnabled: true })).toThrow('Freeze-frame')
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('freeze-frame')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'freeze-frame', label: 'Freeze frame', available: true },
      ],
    })).toBeNull()

    undoComposition()
    const restored = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === clipId)
    expect(restored).toMatchObject({ playbackMode: { mode: 'forward' }, sourceAudioEnabled: true })

    updateCompositionClipSpeed(clipId, 0.5)
    updateCompositionFrameInterpolation(clipId, 'optical_flow')
    expect(() => freezeCompositionClipAtPlayhead(clipId)).toThrow('optical flow')
  })

  it('authors stabilization through history and gates the request-specific capability', () => {
    const clipId = addMediaInfoToComposition(video)
    updateCompositionStabilization(clipId, { mode: 'deshake', radiusX: 32, radiusY: 64 })

    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({
      stabilization: { mode: 'deshake', radiusX: 32, radiusY: 64 },
    })
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('не объявил поддержку stabilization')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'stabilization', label: 'Stabilization', available: false, reason: 'deshake missing' },
      ],
    })).toBe('deshake missing')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'stabilization', label: 'Stabilization', available: true },
      ],
    })).toBeNull()

    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    expect(() => freezeCompositionClipAtPlayhead(clipId)).toThrow('stabilization')
    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'video')!
    expect(() => toggleCompositionTrackFlag(track.id, 'hidden')).toThrow(/Deshake|Stabilization/)
    undoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ stabilization: { mode: 'disabled' } })
    redoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({
      stabilization: { mode: 'deshake', radiusX: 32, radiusY: 64 },
    })
  })

  it('relinks document and local media metadata atomically with undo/redo', () => {
    const clipId = addMediaInfoToComposition(video)
    const replacement: MediaEntry = {
      id: 'source-video-relinked',
      kind: 'source',
      filename: 'replacement.mov',
      url: '/files/sources/replacement.mov',
      mediaType: 'video',
      duration: 12,
      width: 3840,
      height: 2160,
      fps: 60,
      vcodec: 'hevc',
      acodec: 'aac',
      sizeBytes: 123_456,
      createdAt: 42,
    }

    relinkCompositionSource(video.id, replacement)
    expect(compositionState.document.sources[video.id]).toBeUndefined()
    expect(compositionState.document.sources[replacement.id]).toMatchObject({
      id: replacement.id,
      fps: 60,
      vcodec: 'hevc',
      acodec: 'aac',
      hasAudio: true,
    })
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ id: clipId, sourceId: replacement.id })
    expect(compositionState.media[video.id]).toBeUndefined()
    expect(compositionState.media[replacement.id]).toMatchObject({
      url: replacement.url,
      filename: replacement.filename,
      duration: 12,
      width: 3840,
      height: 2160,
      fps: 60,
      vcodec: 'hevc',
      acodec: 'aac',
    })

    undoComposition()
    expect(compositionState.document.sources[video.id]).toBeTruthy()
    expect(compositionState.media[video.id]?.url).toBe(video.url)
    expect(compositionState.media[replacement.id]).toBeUndefined()
    redoComposition()
    expect(compositionState.document.sources[replacement.id]).toBeTruthy()
    expect(compositionState.media[replacement.id]?.url).toBe(replacement.url)

    expect(compositionSourceFromLibraryEntry({ ...replacement, id: 'unknown-audio', acodec: undefined }).hasAudio).toBe(false)
  })

  it('creates deterministic multicam program cuts, records exact switches, and preserves foreign clips on rebuild', () => {
    addMediaInfoToComposition(video)
    addMediaInfoToComposition({
      ...video,
      id: 'source-cam-b',
      url: '/files/sources/cam-b.mp4',
      filename: 'cam-b.mp4',
      duration: 12,
    })
    const clipC = addMediaInfoToComposition({
      ...video,
      id: 'source-cam-c',
      url: '/files/sources/cam-c.mp4',
      filename: 'cam-c.mp4',
      duration: 11,
    })
    const groupId = createCompositionMulticamGroup({
      name: 'Concert',
      timelineStartTicks: 0,
      durationTicks: 10 * COMPOSITION_TIME_BASE,
      audioSourceId: 'source-cam-b',
      angles: [
        { sourceId: 'source-video', label: 'Wide', sourceTickAtGroupStart: 0 },
        { sourceId: 'source-cam-b', label: 'Close', sourceTickAtGroupStart: 0 },
        { sourceId: 'source-cam-c', label: 'Side', sourceTickAtGroupStart: 0 },
      ],
    })
    let group = compositionMulticamGroups().find((candidate) => candidate.id === groupId)!
    expect(group.angles).toHaveLength(3)
    expect(group.switches).toHaveLength(1)
    const programTrack = compositionState.document.tracks.find((track) => track.id === group.videoTrackId)!
    const masterTrack = compositionState.document.tracks.find((track) => track.id === group.audioTrackId)!
    expect(programTrack.clips).toEqual([
      expect.objectContaining({ id: group.switches[0]!.clipId, sourceId: 'source-video', sourceAudioEnabled: false }),
    ])
    expect(masterTrack.clips).toEqual([
      expect.objectContaining({ id: group.audioClipId, sourceId: 'source-cam-b', sourceInTicks: 0, sourceOutTicks: 10 * COMPOSITION_TIME_BASE }),
    ])

    moveCompositionClip(clipC, group.videoTrackId, 10 * COMPOSITION_TIME_BASE, false)
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    const secondAngle = group.angles[1]!
    recordCompositionMulticamSwitch(group.id, secondAngle.id)
    group = compositionMulticamGroups().find((candidate) => candidate.id === groupId)!
    expect(group.switches.map((change) => ({ tick: change.timelineTick, angleId: change.angleId }))).toEqual([
      { tick: 0, angleId: group.angles[0]!.id },
      { tick: 2 * COMPOSITION_TIME_BASE, angleId: secondAngle.id },
    ])
    let rebuiltProgram = compositionState.document.tracks.find((track) => track.id === group.videoTrackId)!
    expect(rebuiltProgram.clips).toEqual([
      expect.objectContaining({ sourceId: 'source-video', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 2 * COMPOSITION_TIME_BASE }),
      expect.objectContaining({ sourceId: 'source-cam-b', timelineStartTicks: 2 * COMPOSITION_TIME_BASE, sourceInTicks: 2 * COMPOSITION_TIME_BASE, sourceOutTicks: 10 * COMPOSITION_TIME_BASE }),
      expect.objectContaining({ id: clipC, timelineStartTicks: 10 * COMPOSITION_TIME_BASE }),
    ])
    const ownedIds = group.switches.map((change) => change.clipId)
    rebuildCompositionMulticamGroup(group.id)
    rebuiltProgram = compositionState.document.tracks.find((track) => track.id === group.videoTrackId)!
    expect(rebuiltProgram.clips.filter((clip) => ownedIds.includes(clip.id)).map((clip) => clip.id)).toEqual(ownedIds)
    expect(rebuiltProgram.clips.some((clip) => clip.id === clipC)).toBe(true)

    undoComposition()
    expect(compositionMulticamGroups()[0]!.switches).toHaveLength(1)
    expect(compositionState.document.tracks
      .find((track) => track.id === group.videoTrackId)?.clips.some((clip) => clip.id === clipC)).toBe(true)
    redoComposition()
    expect(compositionMulticamGroups()[0]!.switches).toHaveLength(2)
    expect(compositionState.document.tracks
      .find((track) => track.id === group.videoTrackId)?.clips.some((clip) => clip.id === clipC)).toBe(true)
  })

  it('aligns three synthetic common-clap waveforms within one composition frame', async () => {
    const sources = [
      { id: 'sync-a', shift: 0 },
      { id: 'sync-b', shift: 5 },
      { id: 'sync-c', shift: -4 },
    ] as const
    for (const source of sources) {
      addMediaInfoToComposition({
        ...video,
        id: source.id,
        url: `/files/sources/${source.id}.mp4`,
        filename: `${source.id}.mp4`,
        duration: 1.2,
      })
    }
    const summaries = Object.fromEntries(sources.map((source) => {
      const envelope = Array.from({ length: 120 }, (_, index) => {
        const baseIndex = index - source.shift
        return baseIndex === 30 ? 1 : baseIndex === 31 ? 0.65 : baseIndex === 72 ? 0.8 : baseIndex === 96 ? 0.45 : 0.01
      })
      return [`/files/sources/${source.id}.mp4`, {
        durationSeconds: 1.2,
        sampleRate: 48_000,
        buckets: envelope.map((value) => ({ min: -value, max: value, rms: value })),
      }]
    }))
    const estimates = await estimateCompositionMulticamSync(
      sources.map((source) => source.id),
      { load: async (url) => summaries[url]! },
    )
    const alignedClapTimes = estimates.map((estimate) => {
      const source = sources.find((candidate) => candidate.id === estimate.sourceId)!
      const clapSourceTick = Math.round(((30 + source.shift) / 100) * COMPOSITION_TIME_BASE)
      return clapSourceTick - estimate.sourceTickAtGroupStart
    })
    expect(Math.max(...alignedClapTimes) - Math.min(...alignedClapTimes))
      .toBeLessThanOrEqual(Math.round(COMPOSITION_TIME_BASE / 30))
  })

  it('imports UTF-8 SRT atomically into a new text track and exports it locally', () => {
    addMediaInfoToComposition(video)
    const count = importSrtToComposition(
      '1\n00:00:00,000 --> 00:00:01,500\nПривет 👋\n\n2\n00:00:02,000 --> 00:00:03,000\nМир',
    )

    expect(count).toBe(2)
    const textTrack = compositionState.document.tracks.find((track) => track.kind === 'text')!
    expect(textTrack.clips.map((clip) => clip.text)).toEqual(['Привет 👋', 'Мир'])
    const exported = exportSelectedTextTrackSrt()
    expect(exported.filename).toMatch(/\.srt$/)
    expect(exported.text).toContain('Привет 👋')
    expect(exported.text).toContain('00:00:02,000 --> 00:00:03,000')

    const before = JSON.stringify(compositionState.document)
    expect(importSrtToComposition(
      '1\n00:00:00,500 --> 00:00:01,000\nOverlap', textTrack.id,
    )).toBe(1)
    const lanes = compositionState.document.tracks.filter((track) => track.kind === 'text')
    expect(lanes).toHaveLength(2)
    expect(lanes[1]!.clips[0]).toMatchObject({ text: 'Overlap', timelineStartTicks: 500_000, durationTicks: 500_000 })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it('keeps title tracks separate and skips locked subtitle lanes atomically', () => {
    const titleId = addCompositionTrack('text')
    importSrtToComposition('1\n00:00:00,000 --> 00:00:01,000\nCaption')
    const captionId = compositionState.document.metadata!.captionTextTracks![0]!
    expect(captionId).not.toBe(titleId)
    expect(compositionState.document.tracks.find((track) => track.id === titleId)!.clips).toHaveLength(0)
    toggleCompositionTrackFlag(captionId, 'locked')
    const before = JSON.stringify(compositionState.document)
    expect(() => importSrtToComposition('1\n00:00:02,000 --> 00:00:03,000\nLocked', captionId)).toThrow('заблокирована')
    expect(JSON.stringify(compositionState.document)).toBe(before)
    importSrtToComposition('1\n00:00:02,000 --> 00:00:03,000\nNew lane')
    expect(compositionState.document.metadata!.captionTextTracks).toHaveLength(2)
    expect(compositionState.document.tracks.find((track) => track.id === captionId)!.clips).toHaveLength(1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it('packs nested and adjacent cues into existing caption gaps with one Undo', () => {
    const before = JSON.stringify(compositionState.document)
    importSrtToComposition('1\n00:00:00,000 --> 00:00:04,000\nLong\n\n2\n00:00:01,000 --> 00:00:02,000\nInner\n\n3\n00:00:02,000 --> 00:00:03,000\nAdjacent')
    const lanes = compositionState.document.tracks.filter((track) => track.kind === 'text')
    expect(lanes.map((track) => track.clips.map((clip) => clip.text))).toEqual([['Long'], ['Inner', 'Adjacent']])
    expect(compositionState.document.metadata?.captionTextTracks).toEqual(lanes.map((track) => track.id))
    const packed = JSON.stringify(compositionState.document)
    importSrtToComposition('1\n00:00:04,000 --> 00:00:05,000\nReuse')
    expect(compositionState.document.tracks.filter((track) => track.kind === 'text')).toHaveLength(2)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(packed)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it.each([
    ['VTT', 'WEBVTT\n\n00:01.125 --> 00:02.500\nHello'],
    ['ASS', '[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.12,0:00:02.50,Hello'],
  ])('imports %s through the same atomic edit and Undo', (_format, body) => {
    const before = JSON.stringify(compositionState.document)
    expect(importSrtToComposition(body)).toBe(1)
    expect(compositionState.document.tracks.find((track) => track.kind === 'text')?.clips[0])
      .toMatchObject({ text: 'Hello', timelineStartTicks: _format === 'VTT' ? 1_125_000 : 1_120_000 })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    const invalid = _format === 'VTT' ? `${body}\n\ninvalid` : `${body}\nDialogue: 0:00:03.00,0:00:02.00,Invalid`
    expect(() => importSrtToComposition(invalid)).toThrow()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it('applies one caption style to its whole text track as one undo step', () => {
    importSrtToComposition(
      '1\n00:00:00,000 --> 00:00:01,000\nOne\n\n2\n00:00:01,000 --> 00:00:02,000\nTwo',
    )
    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'text')!
    const [first, second] = track.clips
    updateCompositionTextStyle(first!.id, { color: '#123456FF', fontSizePx: 72 })
    applyCompositionTextStyleToTrack(first!.id)

    expect(compositionState.document.tracks.find((candidate) => candidate.id === track.id)?.clips)
      .toMatchObject([
        { style: { color: '#123456FF', fontSizePx: 72 } },
        { style: { color: '#123456FF', fontSizePx: 72 } },
      ])
    undoComposition()
    expect(compositionState.document.tracks.find((candidate) => candidate.id === track.id)?.clips)
      .toMatchObject([
        { style: { color: '#123456FF', fontSizePx: 72 } },
        { id: second!.id, style: { color: '#FFFFFFFF', fontSizePx: 48 } },
      ])
  })

  it('commits speed ramp authoring once with undo, redo, autosave and capability gating', () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests({
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => { values.set(key, value) },
    })
    const clipId = addMediaInfoToComposition(video)
    const beforeDepth = compositionState.history.past.length
    updateCompositionSpeedRamp(clipId, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: 5 * COMPOSITION_TIME_BASE, speed: 2 },
        { sourceProgressTick: 10 * COMPOSITION_TIME_BASE, speed: 2 },
      ],
      audioPolicy: 'preserve_pitch',
    })

    expect(compositionState.history.past).toHaveLength(beforeDepth + 1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({
      speed: 1,
      speedRamp: { interpolation: 'hold', audioPolicy: 'preserve_pitch' },
    })
    expect(getCompositionExportUnavailableReason(capabilities)).toContain('speed-ramp')
    expect(getCompositionExportUnavailableReason({
      ...capabilities,
      features: [
        ...capabilities.features!,
        { id: 'speed-ramp', label: 'Speed ramp', available: true },
      ],
    })).toBeNull()

    undoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]).not.toHaveProperty('speedRamp')
    redoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]).toHaveProperty('speedRamp')
    flushCompositionAutosaveForTests()
    expect(activeStoredDraft<{
      document: { tracks: Array<{ clips: Array<{ speedRamp?: unknown }> }> }
    }>(values).document.tracks[0].clips[0].speedRamp)
      .toMatchObject({ interpolation: 'hold', audioPolicy: 'preserve_pitch' })

    const trackId = compositionState.document.tracks[0]!.id
    toggleCompositionTrackFlag(trackId, 'locked')
    const snapshot = JSON.stringify(compositionState.document)
    expect(() => updateCompositionSpeedRamp(clipId, undefined)).toThrow('Дорожка заблокирована')
    expect(JSON.stringify(compositionState.document)).toBe(snapshot)
  })

  it('migrates the active global v1 draft into a stable project slot and clears cross-document history', () => {
    addMediaInfoToComposition(video)
    const document = JSON.parse(JSON.stringify(compositionState.document)) as Composition
    const values = new Map<string, string>([[LEGACY_DRAFT_KEY, JSON.stringify({
      document,
      media: compositionState.media,
      projectId: 'project-a',
      projectName: 'Локальный A',
      exportSettings: { profile: { container: 'webm', codec: 'vp9' }, qualityTier: 'compact' },
    })]])
    setCompositionStorageForTests(memoryStorage(values))

    reloadCompositionDraftForTests()

    const collection = JSON.parse(values.get(DRAFTS_KEY)!)
    expect(collection.activeProjectId).toBe('project-a')
    expect(collection.projects['project-a'].projectName).toBe('Локальный A')
    expect(compositionState.projectId).toBe('project-a')
    expect(compositionState.document).toEqual(document)
    expect(compositionState.history.past).toHaveLength(0)
    expect(compositionState.history.future).toHaveLength(0)
  })

  it('preserves independent unsaved and per-project edits across New/Open switches', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    const serverDocument = JSON.parse(JSON.stringify(compositionState.document)) as Composition
    vi.mocked(api.getCompositionProject).mockImplementation(async (id) => ({
      id,
      name: `Server ${id}`,
      schemaVersion: 2,
      mode: 'composition',
      document: serverDocument,
      sourceIds: [],
      revision: 1,
      createdAt: 1,
      updatedAt: 1,
    }))

    addMediaInfoToComposition(video)
    setCompositionProjectName('Мой несохранённый монтаж')
    updateCompositionExportSettings({ qualityTier: 'compact' })
    const unsavedDocument = JSON.stringify(compositionState.document)
    await openCompositionProject('project-a')
    addCompositionMarkerAtPlayhead('A-local')
    setCompositionProjectName('A local')
    await openCompositionProject('project-b')
    addCompositionMarkerAtPlayhead('B-local')

    newComposition()
    expect(compositionState.projectId).toBeNull()
    expect(compositionState.projectName).toBe('Мой несохранённый монтаж')
    expect(JSON.stringify(compositionState.document)).toBe(unsavedDocument)
    expect(compositionState.media['source-video']).toMatchObject({ filename: 'video.mp4' })
    expect(compositionRenderOutput().qualityTier).toBe('compact')
    expect(compositionState.history.past).toHaveLength(0)

    const beforeGuard = JSON.stringify(compositionState.document)
    newComposition()
    expect(JSON.stringify(compositionState.document)).toBe(beforeGuard)
    expect(compositionState.save.error).toBe('Сначала сохраните текущую композицию')

    await openCompositionProject('project-a')
    expect(compositionState.projectName).toBe('A local')
    expect(compositionMarkerList().map((marker) => marker.label)).toEqual(['A-local'])
    expect(compositionState.history.past).toHaveLength(0)
    await openCompositionProject('project-b')
    expect(compositionMarkerList().map((marker) => marker.label)).toEqual(['B-local'])
  })

  it('promotes a created unsaved draft to its server project id without keeping a stale new slot', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    addMediaInfoToComposition(video)
    setCompositionProjectName('Черновик')
    const document = JSON.parse(JSON.stringify(compositionState.document)) as Composition
    const historyDepth = compositionState.history.past.length
    vi.mocked(api.createCompositionProject).mockResolvedValue({
      id: 'created-project',
      name: 'Сохранённый проект',
      schemaVersion: 2,
      mode: 'composition',
      document,
      sourceIds: ['source-video'],
      revision: 1,
      createdAt: 1,
      updatedAt: 2,
    })

    await saveCompositionProject()

    const collection = JSON.parse(values.get(DRAFTS_KEY)!)
    expect(collection.activeProjectId).toBe('created-project')
    expect(collection.unsaved).toBeUndefined()
    expect(collection.projects['created-project']).toMatchObject({
      projectId: 'created-project',
      projectName: 'Сохранённый проект',
      dirty: false,
    })
    expect(compositionState.history.past).toHaveLength(historyDepth)

    newComposition()
    expect(compositionState.projectId).toBeNull()
    expect(compositionState.document.tracks).toEqual([])
    expect(compositionState.history.past).toHaveLength(0)
  })

  it('saves against the opened server revision and persists the incremented revision', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    const document = JSON.parse(JSON.stringify(compositionState.document)) as Composition
    const opened: api.CompositionProjectDto = {
      id: 'shared-project',
      name: 'Shared cut',
      schemaVersion: 2,
      mode: 'composition',
      document,
      sourceIds: [],
      revision: 4,
      createdAt: 1,
      updatedAt: 2,
    }
    vi.mocked(api.getCompositionProject).mockResolvedValue(opened)
    vi.mocked(api.updateCompositionProject).mockResolvedValue({
      ...opened,
      name: 'Shared cut v2',
      revision: 5,
      updatedAt: 3,
    })

    await openCompositionProject(opened.id)
    setCompositionProjectName('Shared cut v2')
    await saveCompositionProject()

    expect(api.updateCompositionProject).toHaveBeenCalledWith(
      opened.id,
      expect.objectContaining({ baseRevision: 4, name: 'Shared cut v2' }),
      'session-token',
    )
    expect(compositionState.projectRevision).toBe(5)
    expect(activeStoredDraft<{ projectRevision: number }>(values).projectRevision).toBe(5)
  })

  it('auto-refreshes a clean project but preserves dirty edits on a remote revision', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    const opened: api.CompositionProjectDto = {
      id: 'remote-project',
      name: 'Revision one',
      schemaVersion: 2,
      mode: 'composition',
      document: JSON.parse(JSON.stringify(compositionState.document)) as Composition,
      sourceIds: [],
      revision: 1,
      createdAt: 1,
      updatedAt: 1,
    }
    vi.mocked(api.getCompositionProject).mockResolvedValue(opened)
    await openCompositionProject(opened.id)
    vi.mocked(api.getCompositionProjectIfChanged).mockResolvedValueOnce({
      ...opened,
      name: 'Revision two',
      revision: 2,
      updatedAt: 2,
    })

    await expect(refreshOpenCompositionProject()).resolves.toBe('updated')
    expect(compositionState.projectName).toBe('Revision two')
    expect(compositionState.projectRevision).toBe(2)

    setCompositionProjectName('Local unsaved name')
    vi.mocked(api.getCompositionProjectIfChanged).mockResolvedValueOnce({
      ...opened,
      name: 'Revision three',
      revision: 3,
      updatedAt: 3,
    })
    await expect(refreshOpenCompositionProject()).resolves.toBe('conflict')
    expect(compositionState.projectName).toBe('Local unsaved name')
    expect(compositionState.projectRevision).toBe(2)
    expect(compositionState.save.error).toContain('другом устройстве')
  })

  it('refreshes background project additions and deletions from the list snapshot', async () => {
    const project: api.CompositionProjectDto = {
      id: 'background-project',
      name: 'Remote project',
      schemaVersion: 2,
      mode: 'composition',
      document: JSON.parse(JSON.stringify(compositionState.document)) as Composition,
      sourceIds: [],
      revision: 1,
      createdAt: 1,
      updatedAt: 1,
    }
    vi.mocked(api.getCompositionProjectsIfChanged)
      .mockResolvedValueOnce({ etag: '"projects-one"', projects: [project] })
      .mockResolvedValueOnce({ etag: '"projects-empty"', projects: [] })

    await expect(refreshCompositionProjects()).resolves.toBe('updated')
    expect(compositionState.projects.map((item) => item.id)).toEqual([project.id])
    await expect(refreshCompositionProjects()).resolves.toBe('updated')
    expect(compositionState.projects).toEqual([])
    expect(api.getCompositionProjectsIfChanged).toHaveBeenLastCalledWith(
      'session-token',
      '"projects-one"',
    )
  })

  it('refuses New while a project save is in flight', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    addMediaInfoToComposition(video)
    const document = JSON.parse(JSON.stringify(compositionState.document)) as Composition
    let resolveSave!: (project: api.CompositionProjectDto) => void
    vi.mocked(api.createCompositionProject).mockImplementation(() => new Promise((resolve) => {
      resolveSave = resolve
    }))

    const saving = saveCompositionProject()
    newComposition()
    expect(compositionState.projectId).toBeNull()
    expect(compositionState.document).toEqual(document)
    expect(compositionState.save.error).toBe('Дождитесь завершения сохранения проекта')

    resolveSave({
      id: 'saved-after-wait',
      name: 'Saved',
      schemaVersion: 2,
      mode: 'composition',
      document,
      sourceIds: ['source-video'],
      revision: 1,
      createdAt: 1,
      updatedAt: 2,
    })
    await saving
    expect(compositionState.projectId).toBe('saved-after-wait')
  })

  it('reloads the active draft, ignores a stale Open response, and keeps the draft on failure', async () => {
    const values = new Map<string, string>()
    setCompositionStorageForTests(memoryStorage(values))
    addMediaInfoToComposition(video)
    setCompositionProjectName('Reload me')
    flushCompositionAutosaveForTests()
    const expected = JSON.stringify(compositionState.document)

    resetCompositionForTests()
    reloadCompositionDraftForTests()
    expect(compositionState.projectName).toBe('Reload me')
    expect(JSON.stringify(compositionState.document)).toBe(expected)
    expect(compositionState.history.past).toHaveLength(0)

    let resolveA!: (project: api.CompositionProjectDto) => void
    let resolveB!: (project: api.CompositionProjectDto) => void
    vi.mocked(api.getCompositionProject).mockImplementation((id) => new Promise((resolve) => {
      if (id === 'project-a') resolveA = resolve
      else resolveB = resolve
    }))
    const project = (id: string): api.CompositionProjectDto => ({
      id,
      name: id,
      schemaVersion: 2,
      mode: 'composition',
      document: JSON.parse(JSON.stringify(compositionState.document)) as Composition,
      sourceIds: ['source-video'],
      revision: 1,
      createdAt: 1,
      updatedAt: 1,
    })
    const openingA = openCompositionProject('project-a')
    const openingB = openCompositionProject('project-b')
    resolveB(project('project-b'))
    await openingB
    resolveA(project('project-a'))
    await openingA
    expect(compositionState.projectId).toBe('project-b')

    vi.mocked(api.getCompositionProject).mockRejectedValueOnce(new Error('offline'))
    await openCompositionProject('missing')
    expect(compositionState.projectId).toBe('project-b')
    expect(compositionState.save.error).toBe('offline')
  })
})


it('opens and saves nested projects without overwriting the local flat draft', async () => {
  addMediaInfoToComposition(video)
  setCompositionProjectName('Local draft')
  const original = JSON.stringify(compositionState.document)
  const originalId = compositionState.projectId
  const document: api.NativeNestedProject = {
    schemaVersion: 1, root: 'root', compositions: {
      root: { content: buildCompositionRenderRequest(compositionState.document).composition, instances: [], durationUs: 1000000 },
    },
  }
  const project: api.NestedCompositionProjectDto = { id: 'nested', name: 'Nested', schemaVersion: 2, mode: 'composition', document, sourceIds: [], revision: 7, createdAt: 1, updatedAt: 1 }
  vi.mocked(api.getCompositionProject).mockResolvedValue(project as unknown as api.CompositionProjectDto)
  await openCompositionProject('nested')
  expect(nestedComposition.document).toEqual(document)
  expect(compositionState.projectId).toBe(originalId)
  expect(JSON.stringify(compositionState.document)).toBe(original)
  nestedComposition.name = 'Renamed nested'
  vi.mocked(api.updateNestedCompositionProject).mockResolvedValue({ ...project, name: 'Renamed nested', revision: 8 })
  await saveCompositionProject()
  expect(api.updateNestedCompositionProject).toHaveBeenCalledWith('nested', { document, name: 'Renamed nested', baseRevision: 7 }, 'session-token')
  expect(api.updateCompositionProject).not.toHaveBeenCalled()
  expect(api.createCompositionProject).not.toHaveBeenCalled()
  expect(nestedComposition.revision).toBe(8)
  vi.mocked(api.updateNestedCompositionProject).mockRejectedValueOnce(new Error('revision conflict'))
  await saveCompositionProject()
  expect(nestedComposition.revision).toBe(8)
  expect(nestedComposition.document).toEqual(document)
  expect(compositionState.save.error).toBe('revision conflict')
  expect(compositionState.save.busy).toBe(false)
  closeNestedComposition()
  expect(JSON.stringify(compositionState.document)).toBe(original)
})


it('DSP chain edits clone parameters, undo/redo, reload draft and reject locked clips', () => {
  setCompositionStorageForTests(memoryStorage(new Map<string, string>()))
  const id = addMediaInfoToComposition({ id: 'dsp-audio', url: '/files/sources/dsp.wav', filename: 'dsp.wav', mediaType: 'audio', duration: 5, width: 0, height: 0, acodec: 'pcm_s16le' })
  const current = () => compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(track => track.clips).find(clip => clip.id === id) as import('../composition/types').AudioClip
  const params = { gain: -6 }
  updateCompositionAudioMix(id, { dspEffects: [{ id: 'amplify', params }, { id: 'invert', bypass: true }] })
  params.gain = 12
  expect(current().dspEffects).toEqual([{ id: 'amplify', params: { gain: -6 } }, { id: 'invert', bypass: true }])
  undoComposition()
  expect(current().dspEffects).toBeUndefined()
  redoComposition()
  expect(current().dspEffects?.[0]?.params?.gain).toBe(-6)
  flushCompositionAutosaveForTests()
  resetCompositionForTests(); reloadCompositionDraftForTests()
  expect(current().dspEffects?.[0]?.params?.gain).toBe(-6)
  setCompositionClipLocked(id, true)
  const before = JSON.stringify(compositionState.document)
  expect(() => updateCompositionAudioMix(id, { dspEffects: [] })).toThrow('заблокирован')
  expect(JSON.stringify(compositionState.document)).toBe(before)
})


it('persists ordinary track sync-lock through history and draft reload without changing media', () => {
  const values = new Map<string, string>()
  setCompositionStorageForTests({
    getItem: key => values.get(key) ?? null,
    setItem: (key, value) => { values.set(key, value) },
  })
  addMediaInfoToComposition(video)
  const track = compositionState.document.tracks[0]!
  const clips = JSON.stringify(track.clips)
  const depth = compositionState.history.past.length
  expect(track.syncLock ?? true).toBe(true)
  toggleCompositionTrackFlag(track.id, 'syncLock')
  expect(compositionState.document.tracks[0]!.syncLock).toBe(false)
  expect(compositionState.history.past.length).toBe(depth + 1)
  expect(JSON.stringify(compositionState.document.tracks[0]!.clips)).toBe(clips)
  undoComposition()
  expect(compositionState.document.tracks[0]!.syncLock ?? true).toBe(true)
  redoComposition()
  expect(compositionState.document.tracks[0]!.syncLock).toBe(false)
  flushCompositionAutosaveForTests()
  resetCompositionForTests()
  reloadCompositionDraftForTests()
  expect(compositionState.document.tracks[0]!.syncLock).toBe(false)
  expect(JSON.stringify(compositionState.document.tracks[0]!.clips)).toBe(clips)
  toggleCompositionTrackFlag(track.id, 'syncLock')
  expect(compositionState.document.tracks[0]!.syncLock).toBe(true)
})


it('commits global media/caption insertion once and restores both on Undo/Redo', () => {
  addMediaInfoToComposition(video)
  const track = compositionState.document.tracks[0]!
  const original = track.clips[0]!
  if (original.kind !== 'video') throw new Error('video fixture required')
  commitDocument({...compositionState.document,captionTracks:[{id:'captions',name:'Subtitle',clips:[{id:'cue',text:'Retained',cueId:'file',timelineStartTick:1_000_000,timelineEndTick:6_000_000}]}]})
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  insertCompositionPlacements([{trackId:track.id,clip:{...original,id:'inserted',timelineStartTicks:3_000_000,sourceInTicks:0,sourceOutTicks:2_000_000}}])
  expect(compositionState.history.past.length).toBe(depth + 1)
  expect(compositionState.document.tracks[0]!.clips.map(clip=>clip.timelineStartTicks)).toEqual([0,3_000_000,5_000_000])
  expect(compositionState.document.captionTracks![0]!.clips.map(cue=>[cue.timelineStartTick,cue.timelineEndTick])).toEqual([[1_000_000,3_000_000],[5_000_000,8_000_000]])
  const after = JSON.stringify(compositionState.document)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  redoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(after)
})

it('detaches and rejoins audio atomically, retaining mix automation and undo/redo', () => {
  const id = addMediaInfoToComposition(video, { pairAudio: false })
  updateCompositionPlaybackMode(id, { mode: 'reverse' })
  updateCompositionClipSpeed(id, 2)
  setCompositionAutomationKeyframe({ kind: 'audio', clipId: id }, 'gain', 0.8, 1_000_000)
  const original = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const soundId = detachCompositionVideoAudio(id)
  expect(compositionState.document.links?.some(link => link.memberIds.includes(id) && link.memberIds.includes(soundId))).toBe(true)
  updateCompositionClipGain(soundId, 0.6)
  updateCompositionAudioMix(soundId, { pan: -0.25 })
  setCompositionAutomationKeyframe({ kind: 'audio', clipId: soundId }, 'gain', 0.4, 1_000_000)
  const detached: Composition = JSON.parse(JSON.stringify(compositionState.document))
  const sound = detached.tracks.flatMap<import('../composition/types').CompositionClip>(t => t.clips).find(c => c.id === soundId)!
  const beforeJoin = compositionState.history.past.length
  reattachCompositionVideoAudio(soundId)
  expect(compositionState.history.past).toHaveLength(beforeJoin + 1)
  expect(compositionState.ui.selectedClipId).toBe(id)
  const clips = compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(t => t.clips)
  expect(clips.find(c => c.id === soundId)).toBeUndefined()
  expect(clips.find(c => c.id === id)).toMatchObject({ sourceAudioEnabled: true, audioGain: 0.6, audioPan: -0.25, audioAnimation: sound.kind === 'audio' ? sound.audioAnimation : undefined })
  undoComposition(); expect(JSON.stringify(compositionState.document)).toBe(JSON.stringify(detached))
  redoComposition(); expect(compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(t => t.clips).some(c => c.id === soundId)).toBe(false)
  while (compositionState.history.past.length > depth) undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(original)
})

it('saves an unsaved imported nested project through createNested without writing the old flat document',async()=>{
 const {authState}=await import('./auth.svelte');authState.token='session-token'
 const document:api.NativeNestedProject={schemaVersion:1,root:'imported',compositions:{}}
 nestedComposition.document=document;nestedComposition.projectId=null;nestedComposition.name='From Concat'
 vi.mocked(api.createNestedCompositionProject).mockResolvedValue({id:'created-import',name:'From Concat',revision:1,document} as api.NestedCompositionProjectDto)
 await saveCompositionProject();expect(api.createNestedCompositionProject).toHaveBeenCalledWith(expect.objectContaining({name:'From Concat',document}),'session-token');expect(api.createCompositionProject).not.toHaveBeenCalled();expect(api.updateCompositionProject).not.toHaveBeenCalled();expect(nestedComposition.projectId).toBe('created-import');expect(nestedComposition.revision).toBe(1);closeNestedComposition()
})

it('source volume-key edits clone input, survive draft reload and respect clip locking',()=>{
 setCompositionStorageForTests(memoryStorage(new Map<string,string>()))
 const id=addMediaInfoToComposition({id:'source-ride',url:'/files/sources/ride.wav',filename:'ride.wav',mediaType:'audio',duration:1,width:0,height:0,acodec:'pcm_s16le'})
 const current=()=>compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(track=>track.clips).find(clip=>clip.id===id) as import('../composition/types').AudioClip
 const keys=[{at:0,value:0.25},{at:1,value:1,ease:[0.42,-0.5,0.58,1] as [number,number,number,number]}]
 updateCompositionSourceGainKeys(id,keys);keys[1]!.ease![1]=7
 expect(current().concatGainKeys?.[1]!.ease).toEqual([0.42,-0.5,0.58,1]);undoComposition();expect(current().concatGainKeys).toBeUndefined();redoComposition();expect(current().concatGainKeys).toHaveLength(2)
 flushCompositionAutosaveForTests();resetCompositionForTests();reloadCompositionDraftForTests();expect(current().concatGainKeys).toHaveLength(2)
 setCompositionClipLocked(id,true);expect(()=>updateCompositionSourceGainKeys(id,[])).toThrow('заблокирован')
})

it('manual detach and rejoin retain processed video audio and edits made while detached',()=>{
 const id=addMediaInfoToComposition(video,{pairAudio:false})
 compositionState.document={...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,dspEffects:[{id:'concat.echo',params:{delay:0.25}}],audioFadeInTicks:20000000,audioFadeOutTicks:20000000}:clip)}:track)}
 updateCompositionSourceGainKeys(id,[{at:0,value:0.25},{at:1,value:1}])
 const soundId=detachCompositionVideoAudio(id)
 const clips=()=>compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(track=>track.clips)
 expect(clips().find(clip=>clip.id===soundId)).toMatchObject({dspEffects:[{id:'concat.echo',params:{delay:0.25}}],fadeInTicks:20000000,fadeOutTicks:20000000,concatGainKeys:[{value:0.25},{value:1}]})
 updateCompositionAudioMix(soundId,{dspEffects:[{id:'concat.echo',params:{delay:0.5}}]});updateCompositionSourceGainKeys(soundId,[{at:0,value:0.5},{at:1,value:2}])
 reattachCompositionVideoAudio(soundId)
 expect(clips().find(clip=>clip.id===id)).toMatchObject({sourceAudioEnabled:true,dspEffects:[{id:'concat.echo',params:{delay:0.5}}],audioFadeInTicks:20000000,audioFadeOutTicks:20000000,concatGainKeys:[{value:0.5},{value:2}]})
 undoComposition();expect(clips().find(clip=>clip.id===soundId)).toBeDefined();redoComposition();expect(clips().find(clip=>clip.id===soundId)).toBeUndefined()
})

it('edits absolute stream indices without clamping and retains choices on detached audio',()=>{
 const id=addMediaInfoToComposition(video,{pairAudio:false})
 updateCompositionAudioStream(id,2)
 const clips=()=>compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(track=>track.clips)
 expect(clips().find(clip=>clip.id===id)).toMatchObject({audioStream:2})
 for(const index of [-1,1.5,4294967296,NaN])expect(()=>updateCompositionAudioStream(id,index)).toThrow('индекс')
 expect(clips().find(clip=>clip.id===id)).toMatchObject({audioStream:2})
 const sound=detachCompositionVideoAudio(id)!;updateCompositionAudioStream(sound,1);expect(clips().find(clip=>clip.id===sound)).toMatchObject({audioStream:1});expect(clips().find(clip=>clip.id===id)).toMatchObject({audioStream:2})
 setCompositionClipLocked(sound,true);expect(()=>updateCompositionAudioStream(sound,undefined)).toThrow('заблокирован');setCompositionClipLocked(sound,false);updateCompositionAudioStream(sound,undefined);expect(Object.hasOwn(clips().find(clip=>clip.id===sound)!,'audioStream')).toBe(false)
})

it('persists inactive DSP source keys through history, draft reload and native payload',()=>{
 setCompositionStorageForTests(memoryStorage(new Map<string,string>()))
 const id=addMediaInfoToComposition({id:'keyed-audio',url:'/files/sources/keyed.wav',filename:'keyed.wav',mediaType:'audio',duration:1,width:0,height:0,acodec:'pcm_f32le'})
 const keys={semitones:[{at:0,value:-12,ease:[0.42,-0.5,0.58,1] as [number,number,number,number]},{at:1,value:12}]}
 updateCompositionAudioMix(id,{dspEffects:[{id:'concat.pitch',params:{semitones:0},concatSourceKeys:keys}]});keys.semitones[0]!.value=7
 const current=()=>compositionState.document.tracks.flatMap<import('../composition/types').CompositionClip>(track=>track.clips).find(clip=>clip.id===id) as import('../composition/types').AudioClip
 expect(current().dspEffects?.[0]?.concatSourceKeys?.semitones?.[0]?.value).toBe(-12);undoComposition();expect(current().dspEffects?.length??0).toBe(0);redoComposition()
 flushCompositionAutosaveForTests();resetCompositionForTests();reloadCompositionDraftForTests();expect(current().dspEffects?.[0]?.concatSourceKeys?.semitones?.[0]?.value).toBe(-12)
 expect(buildCompositionRenderRequest(compositionState.document).composition.tracks.find(track=>track.kind==='audio')).toMatchObject({clips:[{dspEffects:[{params:{semitones:0},concatSourceKeys:{semitones:[{value:-12},{value:12}]}}]}]});expect(current().dspEffects?.[0]?.animation).toBeUndefined()
})
