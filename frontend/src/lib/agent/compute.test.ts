import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api.js'
import { executeAgentRequest, executeAgentTool } from './runtime.svelte.js'
import { agentSession } from './session.svelte.js'
import { shotAnalysisState, setShotReviewSettings } from '../analysis/shotState.svelte.js'
import { addMediaInfoToComposition } from '../state/composition/media.svelte.js'
import { addCompositionTrack } from '../state/composition/timeline.svelte.js'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests } from '../state/composition/core.svelte.js'
import { undoComposition } from '../state/composition/transport.svelte.js'
import type { ShotScan } from '../analysis/shots.js'

vi.mock('../api.js', async (original) => ({ ...await original<typeof api>(), scanLibraryShots: vi.fn(), measureLibraryShots: vi.fn(), compareLibraryFrames: vi.fn() }))
const scan: ShotScan = { version: 1, sourceHash: 'a'.repeat(64), floor: 0.05, durationTicks: 4_000_000, cuts: [{ tick: 1_000_000, score: 0.8 }, { tick: 3_000_000, score: 0.7 }] }
let clipId: string
beforeEach(() => {
  vi.clearAllMocks()
  setCompositionStorageForTests(null)
  resetCompositionForTests()
  shotAnalysisState.scans = {}; shotAnalysisState.measurements = {}
  vi.mocked(api.scanLibraryShots).mockResolvedValue(scan)
  clipId = addMediaInfoToComposition({ id: 'source', url: '/files/sources/source.mp4', filename: 'source.mp4', duration: 4, width: 1280, height: 720, fps: 30, vcodec: 'h264', acodec: 'aac', mediaType: 'video' })
})

describe('MCP shot analysis', () => {
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
