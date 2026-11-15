import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api.js'
import { executeAgentRequest, executeAgentTool } from './runtime.svelte.js'
import { agentSession } from './session.svelte.js'
import { shotAnalysisState, setShotReviewSettings } from '../analysis/shotState.svelte.js'
import { addMediaInfoToComposition } from '../state/composition/media.svelte.js'
import { addCompositionTrack } from '../state/composition/timeline.svelte.js'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests } from '../state/composition/core.svelte.js'
import { undoComposition } from '../state/composition/transport.svelte.js'
import { clipEndTicks } from '../composition/types.js'
import type { ShotScan } from '../analysis/shots.js'
import { buildCompositionWireDocument } from '../composition/payload.js'
import { closeNestedComposition, nestedComposition } from '../state/nestedComposition.svelte.js'
import { defaultSceneDescriptionPreferences, setSceneDescriptionPreferences } from '../state/sceneDescriptionPreferences.svelte.js'

vi.mock('../api.js', async (original) => ({ ...await original<typeof api>(), scanLibraryShots: vi.fn(), measureLibraryShots: vi.fn(), compareLibraryFrames: vi.fn(), describeLibraryShot: vi.fn(), getSceneDescriptionCatalog: vi.fn() }))
const scan: ShotScan = { version: 1, sourceHash: 'a'.repeat(64), floor: 0.05, durationTicks: 4_000_000, cuts: [{ tick: 1_000_000, score: 0.8 }, { tick: 3_000_000, score: 0.7 }] }
let clipId: string
beforeEach(() => {
  vi.clearAllMocks()
  setSceneDescriptionPreferences(defaultSceneDescriptionPreferences)
  setCompositionStorageForTests(null)
  resetCompositionForTests()
  shotAnalysisState.scans = {}; shotAnalysisState.measurements = {}
  vi.mocked(api.scanLibraryShots).mockResolvedValue(scan)
  clipId = addMediaInfoToComposition({ id: 'source', url: '/files/sources/source.mp4', filename: 'source.mp4', duration: 4, width: 1280, height: 720, fps: 30, vcodec: 'h264', acodec: 'aac', mediaType: 'video' })
})

describe('MCP shot analysis', () => {
  it('uses editor preferences for omitted description and cache arguments while honoring explicit overrides', async () => {
    setSceneDescriptionPreferences({ focus: 'shot-type', language: 'en', fps: 4 })
    const span = { startTicks: 0, endTicks: 4_000_000 }
    vi.mocked(api.describeLibraryShot).mockResolvedValue({ text: 'Wide shot', tags: [] })
    vi.mocked(api.getSceneDescriptionCatalog).mockResolvedValue({ sourceHash: scan.sourceHash, descriptions: [], nextCursor: null })
    const access = { token: 'owner' }
    await executeAgentRequest('agent', 'describe_clip', { clipId }, access)
    expect(api.describeLibraryShot).toHaveBeenLastCalledWith('source', {
      span, expectedSourceHash: scan.sourceHash, focus: 'shot-type', language: 'en', fps: 4,
    }, 'owner', undefined, undefined)
    await executeAgentRequest('agent', 'read_scene_descriptions', { media_id: 'source', language: 'ru' }, access)
    expect(api.getSceneDescriptionCatalog).toHaveBeenLastCalledWith('source', {
      focus: 'shot-type', language: 'ru', fps: 4,
    }, 'owner', undefined, expect.any(AbortSignal))
    await executeAgentRequest('agent', 'read_scene_descriptions', { media_id: 'source' }, access)
    expect(api.getSceneDescriptionCatalog).toHaveBeenLastCalledWith('source', {
      focus: 'shot-type', language: 'en', fps: 4,
    }, 'owner', undefined, expect.any(AbortSignal))
  })
  it('passes the resource view to the read-only catalog without scanning or describing', async () => {
    const cached = { sourceHash: scan.sourceHash, descriptions: [], nextCursor: null }
    vi.mocked(api.getSceneDescriptionCatalog).mockResolvedValue(cached)
    const access = { token: 'owner', spaceId: 'personal' }
    const args = { media_id: 'source', focus: 'shot-type', language: 'en', fps: 0.5, cursor: '1-2' }
    await expect(executeAgentRequest('agent', 'read_scene_descriptions', args, access)).resolves.toEqual({
      sourceId: 'source', focus: 'shot-type', language: 'en', fps: 0.5, ...cached,
    })
    expect(api.getSceneDescriptionCatalog).toHaveBeenCalledWith('source', {
      focus: 'shot-type', language: 'en', fps: 0.5, cursor: '1-2',
    }, 'owner', 'personal', expect.any(AbortSignal))
    await expect(executeAgentRequest('agent', 'read_scene_descriptions', { ...args, startTicks: 1, endTicks: 2 }, access))
      .rejects.toThrow('valid source span or a cursor')
    expect(api.getSceneDescriptionCatalog).toHaveBeenCalledTimes(1)
    expect(api.scanLibraryShots).not.toHaveBeenCalled()
    expect(api.describeLibraryShot).not.toHaveBeenCalled()
  })
  it('returns a source scan without an edit and commits auto-split as one Undo', async () => {
    const previousHistory = compositionState.history.past.length
    const result = await executeAgentRequest('agent', 'analyze_clip', { clipId }) as { rows: unknown[] }
    expect(result.rows).toHaveLength(3)
    expect(compositionState.history.past).toHaveLength(previousHistory)
    await executeAgentRequest('agent', 'auto_split_by_shot', { clipId, expectedRevision: agentSession.revision })
    expect(compositionState.document.tracks[0]?.clips).toHaveLength(3)
    expect(compositionState.history.past).toHaveLength(previousHistory + 1)
    undoComposition()
    expect(compositionState.document.tracks[0]?.clips).toHaveLength(1)
  })
  it('analyzes and compares unplaced library video without adding it to the timeline', async () => {
    const before = JSON.stringify(compositionState.document)
    vi.mocked(api.measureLibraryShots).mockResolvedValue([])
    vi.mocked(api.compareLibraryFrames).mockResolvedValue({ leftTick: 1, rightTick: 2, similarity: 0.5, motion: 0.5 })
    const access = { token: 'owner', spaceId: 'space' }
    const result = await executeAgentRequest('agent', 'measure_shots', { media_id: 'unplaced', disabledCuts: [1_000_000] }, access)
    expect(result).toMatchObject({ sourceId: 'unplaced', rows: [
      { id: '0-3000000', startTicks: 0, endTicks: 3_000_000 },
      { id: '3000000-4000000', startTicks: 3_000_000, endTicks: 4_000_000 },
    ] })
    expect((result as { rows: object[] }).rows[0]).not.toHaveProperty('timelineStartTicks')
    expect(api.scanLibraryShots).toHaveBeenCalledWith('unplaced', 'owner', 'space')
    expect(api.measureLibraryShots).toHaveBeenCalledWith('unplaced', [
      { startTicks: 0, endTicks: 3_000_000 }, { startTicks: 3_000_000, endTicks: 4_000_000 },
    ], 'owner', 'space', scan.sourceHash)
    await executeAgentRequest('agent', 'compare_frames', { media_id: 'unplaced', leftTick: 1, rightTick: 2 }, access)
    expect(api.compareLibraryFrames).toHaveBeenCalledWith('unplaced', 1, 2, 'owner', 'space')
    await expect(executeAgentRequest('agent', 'analyze_clip', { clipId, media_id: 'unplaced' }, access)).rejects.toThrow('exactly one')
    await expect(executeAgentRequest('agent', 'auto_split_by_shot', { media_id: 'unplaced' }, access)).rejects.toThrow('requires clipId')
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })
  it('describes a clip or a library span without making a project edit', async () => {
    const before = JSON.stringify(compositionState.document)
    const history = compositionState.history.past.length
    const result = { text: 'Wide shot', tags: ['city'] }
    vi.mocked(api.describeLibraryShot).mockResolvedValue(result)
    const access = { token: 'owner', spaceId: 'space', expiresAt: Date.now() + 30_000 }
    await expect(executeAgentRequest('agent', 'describe_clip', { clipId }, access)).resolves.toMatchObject({
      sourceId: 'source', span: { startTicks: 0, endTicks: 4_000_000 }, ...result,
    })
    const span = { startTicks: 1_000_000, endTicks: 2_000_000 }
    await expect(executeAgentRequest('agent', 'describe_clip', {
      media_id: 'unplaced', sourceSpan: span, focus: 'shot-type', language: 'en', fps: 2,
    }, access)).resolves.toMatchObject({ sourceId: 'unplaced', span, ...result })
    expect(api.describeLibraryShot).toHaveBeenLastCalledWith('unplaced', {
      span, expectedSourceHash: scan.sourceHash, focus: 'shot-type', language: 'en', fps: 2,
    }, 'owner', 'space', expect.any(AbortSignal))
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(history)
  })

  it('rejects invalid ranges and expired or disconnected descriptions before inference', async () => {
    const access = { token: 'owner' }
    await expect(executeAgentRequest('agent', 'describe_clip', { clipId })).rejects.toThrow('authenticated')
    await expect(executeAgentRequest('agent', 'describe_clip', { clipId, media_id: 'unplaced' }, access)).rejects.toThrow('exactly one')
    await expect(executeAgentRequest('agent', 'describe_clip', {
      clipId, sourceSpan: { startTicks: 0, endTicks: 4_000_001 },
    }, access)).rejects.toThrow('clip source range')
    await expect(executeAgentRequest('agent', 'describe_clip', { clipId }, {
      ...access, expiresAt: Date.now() - 1,
    })).rejects.toThrow('expired')
    let connected = true
    vi.mocked(api.scanLibraryShots).mockImplementationOnce(async () => { connected = false; return scan })
    await expect(executeAgentRequest('agent', 'describe_clip', { media_id: 'unplaced' }, {
      ...access, isActive: () => connected,
    })).rejects.toThrow('disconnected')
    expect(api.describeLibraryShot).not.toHaveBeenCalled()
  })

  it('resolves nested clip scopes and binds the logical clip to the library video', async () => {
    const content = buildCompositionWireDocument(compositionState.document)
    const node = { content, instances: [] }
    const source = content.sources.source!
    nestedComposition.document = {
      schemaVersion: 1, root: 'root', compositions: { root: node, child: node },
      projectMedia: { source: { source, name: 'Source', librarySourceId: 'physical' } },
    }
    nestedComposition.revision = 7
    vi.mocked(api.describeLibraryShot).mockResolvedValue({ text: 'Nested shot', tags: [] })
    const access = { token: 'owner' }
    try {
      await expect(executeAgentRequest('agent', 'describe_clip', {
        clipId, composition_id: 'child',
      }, access)).resolves.toMatchObject({ revision: 7, compositionId: 'child', sourceId: 'physical', text: 'Nested shot' })
      expect(api.scanLibraryShots).toHaveBeenCalledWith('physical', 'owner', undefined, undefined)
      await expect(executeAgentRequest('agent', 'describe_clip', {
        clipId, composition_id: 'missing',
      }, access)).rejects.toThrow('composition not found')
      expect(api.describeLibraryShot).toHaveBeenCalledTimes(1)
    } finally { closeNestedComposition() }
  })
  it('closes gaps when applying discarded shots with ripple and undoes the edit once', async () => {
    const before = JSON.stringify(compositionState.document)
    const previousHistory = compositionState.history.past.length
    const base = { clipId, expectedRevision: agentSession.revision }
    await expect(executeAgentRequest('agent', 'apply_shot_cuts', { ...base, ripple: true })).rejects.toThrow('discarded scene')
    await expect(executeAgentRequest('agent', 'apply_shot_cuts', { ...base, ripple: 'true' })).rejects.toThrow('boolean')
    expect(api.scanLibraryShots).not.toHaveBeenCalled()
    await executeAgentRequest('agent', 'apply_shot_cuts', {
      ...base, ripple: true, discardedRowIds: ['1000000-3000000'],
    })
    expect(compositionState.document.tracks[0]?.clips.map(clip => [clip.timelineStartTicks, clipEndTicks(clip)]))
      .toEqual([[0, 1_000_000], [1_000_000, 2_000_000]])
    expect(compositionState.history.past).toHaveLength(previousHistory + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })
  it('refuses a stale revision, a mid-analysis edit, disconnect and expiration', async () => {
    await expect(executeAgentRequest('agent', 'auto_split_by_shot', { clipId, expectedRevision: -1 })).rejects.toThrow('Project changed')
    vi.mocked(api.scanLibraryShots).mockImplementationOnce(async () => { addCompositionTrack('text'); return scan })
    await expect(executeAgentRequest('agent', 'auto_split_by_shot', { clipId, expectedRevision: agentSession.revision })).rejects.toThrow('during analysis')
    for (const access of [{ expiresAt: Date.now() - 1 }, { isActive: () => false }]) {
      const before = JSON.stringify(compositionState.document)
      await expect(executeAgentRequest('agent', 'auto_split_by_shot', { clipId, expectedRevision: agentSession.revision }, access)).rejects.toThrow('disconnected')
      expect(JSON.stringify(compositionState.document)).toBe(before)
    }
  })
  it('stores review parameters outside Undo and applies them to old history/checkpoints', () => {
    const checkpoint = executeAgentTool('user', 'create_checkpoint', { label: 'Saved' }) as { id: string }
    const historyLength = compositionState.history.past.length
    setShotReviewSettings(0.6, 1_000_000)
    expect(compositionState.history.past).toHaveLength(historyLength)
    expect(compositionState.history.past.every((document) => document.shotReview?.threshold === 0.6)).toBe(true)
    expect(agentSession.checkpoints.find((item) => item.id === checkpoint.id)?.document.shotReview?.threshold).toBe(0.6)
    undoComposition()
    expect(compositionState.document.shotReview?.threshold).toBe(0.6)
  })
})
