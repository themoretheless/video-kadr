import {agentActivity,resetAgentActivity} from './activity.svelte'
import { beforeEach, describe, expect, it } from 'vitest'
import { executeAgentTool } from './runtime.svelte.js'
import { agentSession } from './session.svelte.js'
import { addCompositionTrack, setCompositionRoleGain } from '../state/composition/timeline.svelte.js'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests, setCompositionRoleFlags } from '../state/composition/core.svelte.js'
import { undoComposition, redoComposition } from '../state/composition/transport.svelte.js'

const track = { id: 'captions', kind: 'text', name: 'Captions', locked: false, hidden: false, clips: [] }
const clip = { id: 'title', kind: 'text', timelineStartTicks: 0, durationTicks: 2_000_000, text: 'Hello', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } }
const operations = [{ op: 'add_track', track }, { op: 'add_layer', trackId: 'captions', clip }]
const batch = () => ({ expectedRevision: agentSession.revision, operations })

beforeEach(() => {
  setCompositionStorageForTests(null)
  resetCompositionForTests()
  agentSession.log = []
  resetAgentActivity()
})

it('reads RGBA tracks and edits color keys atomically through the agent boundary', () => {
  executeAgentTool('agent-a','apply_batch',batch())
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ mode: 'constant',value: { r: 255,g: 255,b: 255,a: 255 } })
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const args = { expectedRevision: agentSession.revision,operations: [{ op: 'set_color_key',clipId: clip.id,playheadTicks: 500000,value: { r: 255,g: 0,b: 0,a: 128 } },{ op: 'set_color_key',clipId: clip.id,playheadTicks: 1000000,value: { r: 0,g: 0,b: 255,a: 255 } }] }
  executeAgentTool('agent-a','dry_run',args)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',args)
  const after = JSON.stringify(compositionState.document)
  expect(compositionState.history.past).toHaveLength(depth+1)
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ mode: 'keyframes',timeBase: 1000000,keyframes: [{ tick: 0,value: { r: 255,g: 255,b: 255,a: 255 } },{ tick: 500000,value: { r: 255,g: 0,b: 0,a: 128 },t_local_us: 500000,t_us: 500000 },{ tick: 1000000,value: { r: 0,g: 0,b: 255,a: 255 } }] })
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'delete_color_key',clipId: clip.id,tick: 500000 },{ op: 'set_color_key',clipId: clip.id,playheadTicks: 500000,value: { r: 256,g: 0,b: 0,a: 255 } }] })).toThrow('channel')
  expect(JSON.stringify(compositionState.document)).toBe(after)
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_color_key',clipId: clip.id,playheadTicks: 500000,value: 0.5 }] })).toThrow('object')
  expect(JSON.stringify(compositionState.document)).toBe(after)
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
  redoComposition();expect(JSON.stringify(compositionState.document)).toBe(after)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'delete_color_key',clipId: clip.id,tick: 500000 }] })
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(after)
  const moveDepth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'move_color_keys',edits: [{ clipId: clip.id,tick: 500000,newTick: 1000000 },{ clipId: clip.id,tick: 1000000,newTick: 500000 }] }] })
  expect(compositionState.history.past).toHaveLength(moveDepth+1)
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ keyframes: [{ tick: 0 },{ tick: 500000,value: { r: 0,g: 0,b: 255,a: 255 } },{ tick: 1000000,value: { r: 255,g: 0,b: 0,a: 128 } }] })
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(after)
  const curveOps = [
    { op: 'set_color_preset',clipId: clip.id,tick: 0,preset: 'ease_in_quad' },
    { op: 'set_color_extrapolation',clipId: clip.id,side: 'after',extrapolation: 'ping_pong' },
    { op: 'set_color_tangent_mode',clipId: clip.id,tick: 500000,mode: 'auto' },
    { op: 'edit_color_tangents',clipId: clip.id,tick: 500000,side: 'outgoing',point: { x: 0.42,y: -0.4 },continuity: 'broken' },
  ]
  const curveDepth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: curveOps })
  expect(compositionState.history.past).toHaveLength(curveDepth+1)
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ extrapolation: { after: 'ping_pong' },keyframes: [{ tick: 0 },{ tick: 500000,outgoing: { x: 0.42,y: -0.4 },continuity: 'broken' },{ tick: 1000000 }] })
  const preserved = JSON.stringify(compositionState.document)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'edit_color_key',clipId: clip.id,tick: 500000,newTick: -100000,value: { r: 0,g: 255,b: 0,a: 64 } }] })
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ keyframes: [{ tick: -100000,value: { r: 0,g: 255,b: 0,a: 64 },outgoing: { x: 0.42,y: -0.4 },continuity: 'broken' },{ tick: 0 },{ tick: 1000000 }] })
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(preserved)
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'edit_color_key',clipId: clip.id,tick: 500000,newTick: 0 }] })).toThrow()
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'edit_color_key',clipId: clip.id,tick: 500000 }] })).toThrow('requires')
  expect(JSON.stringify(compositionState.document)).toBe(preserved)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_color_interpolation',clipId: clip.id,tick: 500000,interpolation: { kind: 'bezier',p1: [0.2,-0.3],p2: [0.8,1.4] } }] })
  expect(executeAgentTool('agent-a','get_color_track',{ clipId: clip.id })).toMatchObject({ keyframes: [{ tick: 0 },{ tick: 500000,outgoing: { x: 0.2,y: -0.3 } },{ tick: 1000000,incoming: { x: 0.8,y: 1.4 } }] })
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(preserved)
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_color_interpolation',clipId: clip.id,interpolation: { kind: 'elastic',dir: 'out',amplitude: 1.5,period: 0 } }] })).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(preserved)
  const curved = JSON.stringify(compositionState.document)
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [curveOps[0],{ ...curveOps[3],point: { x: 2,y: 0 } }] })).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(curved)
  undoComposition();expect(JSON.stringify(compositionState.document)).toBe(after)
})

describe('visible editor agent tools', () => {
  it('rehearses explicit placements and commits two destinations in one Undo', () => {
    executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [
      ...operations,
      { op: 'add_layer', trackId: 'captions', clip: { ...clip, id: 'second', timelineStartTicks: 4_000_000 } },
      { op: 'add_track', track: { ...track, id: 'destination-a' } },
      { op: 'add_track', track: { ...track, id: 'destination-b' } },
    ] })
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const revision = agentSession.revision
    const args = { expectedRevision: revision, operations: [{ op: 'move_layer_placements', placements: [{ clipId: 'second', trackId: 'destination-b' }, { clipId: 'title', trackId: 'destination-a' }], anchorId: 'title', startTicks: 2_000_000 }] }
    executeAgentTool('agent-a', 'dry_run', args)
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    expect(agentSession.revision).toBe(revision)
    executeAgentTool('agent-a', 'apply_batch', args)
    const after = JSON.stringify(compositionState.document)
    expect(compositionState.document.tracks[1]!.clips[0]).toMatchObject({ id: 'title', timelineStartTicks: 2_000_000 })
    expect(compositionState.document.tracks[2]!.clips[0]).toMatchObject({ id: 'second', timelineStartTicks: 6_000_000 })
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(() => executeAgentTool('agent-a', 'apply_batch', args)).toThrow('Project changed')
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'move_layer_placements', placements: [{ clipId: 'title', trackId: 'captions' }, { clipId: 'second', trackId: 'missing' }], anchorId: 'title', startTicks: 0 }] })).toThrow()
    expect(JSON.stringify(compositionState.document)).toBe(after)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    redoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(after)
  })
  it('rehearses and commits exact canvas settings atomically with revision checks and one Undo', () => {
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const revision = agentSession.revision
    const args = { expectedRevision: revision, operations: [{ op: 'update_canvas', patch: { fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 } } }, { op: 'update_canvas', patch: { width: 1280, height: 720 } }] }
    const preview = executeAgentTool('agent-a', 'dry_run', args) as { document: { canvas: { fpsRate: unknown } } }
    expect(preview.document.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    expect(JSON.stringify(compositionState.document)).toBe(original)
    expect(compositionState.history.past).toHaveLength(depth)
    expect(agentSession.revision).toBe(revision)
    executeAgentTool('agent-a', 'apply_batch', args)
    const committed = JSON.stringify(compositionState.document)
    expect(compositionState.document.canvas.width).toBe(1280)
    expect(compositionState.document.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(() => executeAgentTool('agent-a', 'apply_batch', args)).toThrow('Project changed')
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'update_canvas', patch: { width: 1920 } }, { op: 'update_canvas', patch: { fpsRate: { numerator: 30, denominator: 0 } } }] })).toThrow()
    expect(JSON.stringify(compositionState.document)).toBe(committed)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(original)
    redoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(committed)
  })

  it('rehearses without changing document, history, selection or revision', () => {
    const before = JSON.stringify(compositionState)
    const revision = agentSession.revision
    const result = executeAgentTool('agent-a', 'dry_run', batch()) as { document: { tracks: unknown[] } }
    expect(result.document.tracks).toHaveLength(1)
    expect(JSON.stringify(compositionState)).toBe(before)
    expect(agentSession.revision).toBe(revision)
    expect(agentSession.log.map((entry) => entry.status)).toEqual(['started', 'ok'])
  })
  it('commits a batch once, undo restores all steps and redo restores the result', () => {
    executeAgentTool('agent-a', 'apply_batch', batch())
    expect(compositionState.history.past).toHaveLength(1)
    expect(compositionState.document.tracks[0]?.clips[0]?.id).toBe('title')
    undoComposition()
    expect(compositionState.document.tracks).toHaveLength(0)
    redoComposition()
    expect(compositionState.document.tracks[0]?.clips[0]?.id).toBe('title')
  })
  it('refuses stale revisions and rolls back a failure after a valid first step', () => {
    const stale = batch()
    addCompositionTrack('text')
    expect(() => executeAgentTool('agent-a', 'apply_batch', stale)).toThrow('Project changed')
    const before = JSON.stringify(compositionState)
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [operations[0], { op: 'delete_layer', clipId: 'missing' }] })).toThrow()
    expect(JSON.stringify(compositionState)).toBe(before)
    expect(agentSession.log.at(-1)?.status).toBe('error')
  })
  it('restores checkpoints as undoable edits and clears them on project switch', () => {
    const checkpoint = executeAgentTool('user', 'create_checkpoint', { label: 'Before' }) as { id: string }
    executeAgentTool('agent-a', 'apply_batch', batch())
    executeAgentTool('user', 'restore_checkpoint', { id: checkpoint.id })
    expect(compositionState.document.tracks).toHaveLength(0)
    undoComposition()
    expect(compositionState.document.tracks).toHaveLength(1)
    resetCompositionForTests()
    expect(agentSession.checkpoints).toHaveLength(0)
  })
  it('protects revert paths from other actors, allows the owner and expires', () => {
    executeAgentTool('agent-a', 'apply_batch', batch())
    const checkpoint = executeAgentTool('user', 'create_checkpoint', { label: 'Saved' }) as { id: string }
    executeAgentTool('agent-a', 'set_history_lock', { locked: true, reason: 'Batch in progress' })
    expect(() => undoComposition()).toThrow('История занята')
    expect(() => redoComposition()).toThrow('История занята')
    expect(() => executeAgentTool('agent-b', 'restore_checkpoint', { id: checkpoint.id })).toThrow()
    expect(() => executeAgentTool('agent-b', 'set_history_lock', { locked: false })).toThrow()
    executeAgentTool('agent-a', 'undo', {})
    expect(compositionState.document.tracks).toHaveLength(0)
    agentSession.lock!.expiresAt = Date.now() - 1
    redoComposition()
    expect(compositionState.document.tracks).toHaveLength(1)
    expect(agentSession.lock).toBeNull()
  })
  it('updates layer content and automation through domain validation without changing identity', () => {
    executeAgentTool('agent-a', 'apply_batch', batch())
    executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'update_layer', clipId: 'title', patch: { text: 'Updated', opacity: 0.5 } }] })
    expect(compositionState.document.tracks[0]?.clips[0]).toMatchObject({ text: 'Updated', opacity: 0.5 })
    const before = JSON.stringify(compositionState.document)
    for (const patch of [{ id: 'different' }, { opacity: 2 }, { kind: 'audio' }]) {
      expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'update_layer', clipId: 'title', patch }] })).toThrow()
      expect(JSON.stringify(compositionState.document)).toBe(before)
    }
  })

  it('rehearses and commits exact gap closing, rejecting stale boundaries atomically', () => {
    executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [
      ...operations, { op: 'add_layer', trackId: 'captions', clip: { ...clip, id: 'later', timelineStartTicks: 4_000_000 } },
    ] })
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const args = { expectedRevision: agentSession.revision, operations: [{ op: 'ripple_delete_gap', trackId: 'captions', startTicks: 2_000_000, endTicks: 4_000_000 }] }
    executeAgentTool('agent-a', 'dry_run', args)
    expect(JSON.stringify(compositionState.document)).toBe(before)
    executeAgentTool('agent-a', 'apply_batch', args)
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(2_000_000)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    const closed = JSON.stringify(compositionState.document)
    expect(() => executeAgentTool('agent-a', 'apply_batch', { ...args, expectedRevision: agentSession.revision })).toThrow('exact')
    expect(JSON.stringify(compositionState.document)).toBe(closed)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it('sets clip enabled state through MCP as one undoable edit', () => {
    executeAgentTool('agent-a', 'apply_batch', batch())
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'set_layers_enabled', clipIds: ['title'], enabled: false }] })
    expect(compositionState.document.tracks[0]!.clips[0]!.enabled).toBe(false)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  })

  it('rejects malformed batches and operations targeting locked tracks', () => {
    executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'add_track', track: { ...track, locked: true } }] })
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [operations[1]] })).toThrow()
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [] })).toThrow()
    expect(() => executeAgentTool('agent-a', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'unknown' }] })).toThrow()
  })
})

it('keeps role flags in restored checkpoints while restoring their own gain and preserving other roles', () => {
  setCompositionRoleGain('dialogue', -3)
  const early = executeAgentTool('user', 'create_checkpoint', { label: 'Early role mix' }) as { id: string }
  setCompositionRoleGain('dialogue', -9)
  setCompositionRoleGain('music', -6)
  const late = executeAgentTool('user', 'create_checkpoint', { label: 'Late role mix' }) as { id: string }
  const before = compositionState.history.past.length
  const revision = agentSession.revision
  setCompositionRoleFlags('dialogue', { muted: true })
  setCompositionRoleFlags('dialogue', { solo: true })
  expect(compositionState.history.past.length).toBe(before)
  expect(agentSession.revision).toBe(revision + 2)
  expect(agentSession.checkpoints.find(item => item.id === early.id)!.document.audioRoles?.dialogue).toEqual({ gainDb: -3, muted: true, solo: true })
  expect(agentSession.checkpoints.find(item => item.id === late.id)!.document.audioRoles?.dialogue).toEqual({ gainDb: -9, muted: true, solo: true })
  executeAgentTool('user', 'restore_checkpoint', { id: early.id })
  expect(compositionState.document.audioRoles?.dialogue).toEqual({ gainDb: -3, muted: true, solo: true })
  expect(compositionState.document.audioRoles?.music).toBeUndefined()
  undoComposition()
  expect(compositionState.document.audioRoles?.dialogue).toEqual({ gainDb: -9, muted: true, solo: true })
  expect(compositionState.document.audioRoles?.music).toEqual({ gainDb: -6, muted: false, solo: false })
  resetCompositionForTests()
  expect(compositionState.document.audioRoles).toBeUndefined()
  expect(agentSession.checkpoints).toHaveLength(0)
})

it('does not revise history for unchanged role flags and rejects malformed updates atomically', () => {
  const before = JSON.stringify(compositionState.document)
  const revision = agentSession.revision
  setCompositionRoleFlags('music', {})
  setCompositionRoleFlags('music', { muted: false })
  expect(agentSession.revision).toBe(revision)
  expect(() => setCompositionRoleGain('music', Infinity)).toThrow('Invalid audio role gain')
  expect(() => setCompositionRoleFlags('invalid' as 'music', { muted: true })).toThrow('Invalid audio role flags')
  expect(() => setCompositionRoleFlags('music', { muted: 'yes' as unknown as boolean })).toThrow('Invalid audio role flags')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(agentSession.revision).toBe(revision)
})

it('normalizes Auto on MCP animation replacement and undoes the whole authored patch', () => {
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const tangents = { incoming: { x: 2/3,y: 2/3,mode: 'auto' },outgoing: { x: 1/3,y: 1/3,mode: 'auto' },continuity: 'smooth' }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'update_layer',clipId: 'title',patch: { animation: { x: { mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 0,value: 0,tangents },{ tick: 1_000_000,value: 3,tangents },{ tick: 2_000_000,value: 2,tangents }] } } } } }] })
  const title = compositionState.document.tracks[0]!.clips[0]!
  if (title.kind !== 'text') throw new Error('text expected')
  expect(title.animation!.x).toMatchObject({ track: { keyframes: [{ tangents: { outgoing: { y: 0 } } },{ tangents: { incoming: { y: 1 },outgoing: { y: 0 } } },{ tangents: { incoming: { y: 1 } } }] } })
  expect(compositionState.history.past.length).toBe(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('normalizes admitted Auto tracks when adding a layer without mutating the request', () => {
  const tangents = { incoming: { x: 2/3,y: 2/3,mode: 'auto' },outgoing: { x: 1/3,y: 1/3,mode: 'auto' },continuity: 'smooth' }
  const animated = { ...clip,animation: { x: { mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 0,value: 0,tangents },{ tick: 1_000_000,value: 2,tangents }] } } } }
  const before = JSON.stringify(animated)
  const depth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: 'captions',clip: animated }] })
  const title = compositionState.document.tracks[0]!.clips[0]!
  if (title.kind !== 'text') throw new Error('text expected')
  expect(title.animation!.x).toMatchObject({ track: { keyframes: [{ tangents: { outgoing: { y: 0 } } },{ tangents: { incoming: { y: 1 } } }] } })
  expect(JSON.stringify(animated)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth+1)
  undoComposition()
  expect(compositionState.document.tracks).toHaveLength(0)
})

it('normalizes MCP audio and mask envelopes while preserving untouched tracks', () => {
  compositionState.document = { ...compositionState.document,sources: { media: { id: 'media',kind: 'video',durationTicks: 2_000_000,width: 1280,height: 720,hasAudio: true } } }
  const initial = JSON.stringify(compositionState.document)
  const tangents = { incoming: { x: 2/3,y: 2/3,mode: 'auto' },outgoing: { x: 1/3,y: 1/3,mode: 'auto' },continuity: 'smooth' }
  const curve = { mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 0,value: 0.2,tangents },{ tick: 1_000_000,value: 0.8,tangents },{ tick: 2_000_000,value: 0.4,tangents }] } }
  const mask = { id: 'mask',shape: 'rectangle',x: curve,y: { mode: 'constant',value: 0.5 },width: { mode: 'constant',value: 0.8 },height: { mode: 'constant',value: 0.8 },rotationDegrees: curve,feather: 0,inverted: false }
  const video = { id: 'video',kind: 'video',sourceId: 'media',timelineStartTicks: 0,sourceInTicks: 0,sourceOutTicks: 2_000_000,transform: { x: 0,y: 0,width: 1280,height: 720,fit: 'contain' },opacity: 1,sourceAudioEnabled: true,audioGain: 1,audioAnimation: { gain: curve },masks: [mask] }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track: { id: 'video-track',kind: 'video',name: 'Video',locked: false,hidden: false,muted: false,clips: [] } },{ op: 'add_layer',trackId: 'video-track',clip: video }] })
  const added = compositionState.document.tracks[0]!.clips[0]!
  if (added.kind !== 'video') throw new Error('video required')
  for (const value of [added.audioAnimation!.gain!,added.masks![0]!.x,added.masks![0]!.rotationDegrees!]) expect(value).toMatchObject({ track: { keyframes: [{},{ tangents: { incoming: { y: 1 },outgoing: { y: 0 } } },{}] } })
  const before = JSON.stringify(compositionState.document)
  const audioBefore = JSON.stringify(added.audioAnimation)
  const depth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'update_layer',clipId: 'video',patch: { masks: [mask] } }] })
  const patched = compositionState.document.tracks[0]!.clips[0]!
  if (patched.kind !== 'video') throw new Error('video required')
  expect(JSON.stringify(patched.audioAnimation)).toBe(audioBefore)
  expect(patched.masks![0]!.x).toEqual(added.masks![0]!.x)
  // Replacing identical raw Auto coordinates resolves to the same authored result.
  expect(compositionState.history.past.length).toBe(depth)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(initial)
})

it('authors named keyframe presets through MCP with dry run, linked scale, refusal and exact Undo', () => {
  const scale = { mode: 'keyframes',track: { timeBase: 1_000_000,interpolation: 'linear',keyframes: [{ tick: 0,value: 1 },{ tick: 1_000_000,value: 2 }] } }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,scaleLinked: true,animation: { scaleX: scale,scaleY: structuredClone(scale) } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const args = { expectedRevision: agentSession.revision,operations: [{ op: 'set_keyframe_preset',clipId: clip.id,property: 'scaleX',ticks: [0],preset: 'ease_in_quad' }] }
  executeAgentTool('agent-a','dry_run',args)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  expect(() => executeAgentTool('agent-a','apply_batch',{ ...args,operations: [{ ...args.operations[0],preset: 'unknown' }] })).toThrow('Unknown easing preset')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',args)
  const authored = compositionState.document.tracks[0]!.clips[0]!
  if (authored.kind !== 'text') throw new Error('text required')
  expect(authored.animation!.scaleX).toEqual(authored.animation!.scaleY)
  expect(authored.animation!.scaleX).toMatchObject({ track: { keyframes: [{ interpolation: { kind: 'bezier',p1: [1/3,0],p2: [2/3,1/3] } },{}] } })
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('applies audio and mask presets atomically and refuses missing masks without partial edits', () => {
  compositionState.document = { ...compositionState.document,sources: { media: { id: 'media',kind: 'video',durationTicks: 2_000_000,width: 1280,height: 720,hasAudio: true } } }
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0.2 },{ tick: 1000,value: 0.8 }] } }
  const constant = { mode: 'constant',value: 0.5 }
  const mask = { id: 'mask',shape: 'rectangle',x: curve,y: constant,width: constant,height: constant,feather: 0,inverted: false }
  const video = { id: 'video',kind: 'video',sourceId: 'media',timelineStartTicks: 0,sourceInTicks: 0,sourceOutTicks: 2_000_000,transform: { x: 0,y: 0,width: 1280,height: 720,fit: 'contain' },opacity: 1,sourceAudioEnabled: true,audioGain: 1,audioAnimation: { gain: curve,pan: curve },masks: [mask] }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track: { id: 'v',kind: 'video',name: 'Video',locked: false,hidden: false,muted: false,clips: [] } },{ op: 'add_layer',trackId: 'v',clip: video }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const edits = [{ op: 'set_keyframe_preset',clipId: 'video',envelope: 'audio',property: 'pan',ticks: [0],preset: 'ease_in_quad' },{ op: 'set_keyframe_preset',clipId: 'video',envelope: 'mask',maskId: 'mask',property: 'x',ticks: [0],preset: 'ease_out_quad' }]
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [edits[0],{ ...edits[1],maskId: 'missing' }] })).toThrow('Mask not found')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: edits })
  const changed = compositionState.document.tracks[0]!.clips[0]!
  if (changed.kind !== 'video') throw new Error('video required')
  expect(changed.audioAnimation!.gain).toEqual(curve)
  expect(changed.audioAnimation!.pan).toMatchObject({ track: { keyframes: [{ interpolation: { p1: [1/3,0],p2: [2/3,1/3] } },{}] } })
  expect(changed.masks![0]!.x).toMatchObject({ track: { keyframes: [{ interpolation: { p1: [1/3,2/3],p2: [2/3,1] } },{}] } })
  expect(changed.masks![0]!.y).toEqual(constant)
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it.each(['linear','hold','ease_in_elastic','ease_out_elastic','ease_in_out_elastic','ease_in_bounce','ease_out_bounce','ease_in_out_bounce'])('authors canonical closed-form preset %s in one MCP history entry', async preset => {
  const { EASING_PRESETS } = await import('../composition/easingPresets')
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,animation: { opacity: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'ease_in',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } } } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_keyframe_preset',clipId: clip.id,property: 'opacity',ticks: [0],preset }] })
  const changed = compositionState.document.tracks[0]!.clips[0]!
  if (changed.kind !== 'text' || changed.animation!.opacity!.mode !== 'keyframes') throw new Error('keys required')
  expect(changed.animation!.opacity!.track.keyframes[0]!.interpolation).toEqual(EASING_PRESETS.find(entry => entry.id === preset)!.interpolation)
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('reads effective segment presets and clock coordinates without changing history or authored state', () => {
  const interpolation = { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] }
  const first = { incoming: { x: 0.5,y: -9,mode: 'free' },outgoing: { x: 1/3,y: 0,mode: 'free' },continuity: 'broken' }
  const last = { incoming: { x: 2/3,y: 1/3,mode: 'free' },outgoing: { x: 0.5,y: 9,mode: 'free' },continuity: 'broken' }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,timelineStartTicks: 2_000_000,animation: { opacity: { mode: 'keyframes',track: { timeBase: 1000,interpolation,keyframes: [{ tick: 250,value: 0,tangents: first },{ tick: 1000,value: 1,tangents: last }] } } } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'opacity' }) as { keyframes: Record<string,unknown>[] }
  expect(read.keyframes[0]).toMatchObject({ tick: 250,t_local_us: 250_000,t_us: 2_250_000,preset_id: 'ease_in_quad',segment: { kind: 'bezier',p1: [1/3,0],p2: [2/3,1/3] } })
  expect(read.keyframes[1]).not.toHaveProperty('preset_id')
  expect(read.keyframes[1]).not.toHaveProperty('segment')
  expect(executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' })).toMatchObject({ mode: 'constant',value: 0 })
  expect(() => executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'gain' })).toThrow('Unknown animation property')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
})

it('solves Auto keys through MCP and freezes their coordinates with Free in atomic edits', () => {
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 },{ tick: 2000,value: 0 }] } }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,animation: { x: curve } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const command = { op: 'set_keyframe_tangent_mode',clipId: clip.id,property: 'x',ticks: [0,1000,2000],mode: 'auto' }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [command] })
  const auto = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' }) as { keyframes: { incoming: { x: number;y: number;mode: string };outgoing: { x: number;y: number;mode: string } }[] }
  expect(auto.keyframes[1]).toMatchObject({ incoming: { y: 1,mode: 'auto' },outgoing: { y: 0,mode: 'auto' } })
  expect(compositionState.history.past).toHaveLength(depth+1)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...command,mode: 'free' }] })
  const free = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' }) as typeof auto
  free.keyframes.forEach((key,index) => {
    expect(key.incoming).toEqual({ ...auto.keyframes[index]!.incoming,mode: 'free' })
    expect(key.outgoing).toEqual({ ...auto.keyframes[index]!.outgoing,mode: 'free' })
  })
  expect(compositionState.history.past).toHaveLength(depth+2)
  undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_layer_locked',clipId: clip.id,locked: true }] })
  const locked = JSON.stringify(compositionState.document)
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [command] })).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(locked)
})

it('authors manual tangents and continuity through MCP in one undoable command', () => {
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 },{ tick: 2000,value: 2 }] } }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,animation: { x: curve } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const edit = { op: 'edit_keyframe_tangents',clipId: clip.id,property: 'x',tick: 1000,incoming: { x: 2/3,y: 0.5 },outgoing: { x: 1/3,y: 0.5 },continuity: 'smooth' }
  executeAgentTool('agent-a','dry_run',{ expectedRevision: agentSession.revision,operations: [edit] })
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [edit] })
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' }) as { keyframes: Record<string,unknown>[] }
  expect(read.keyframes[1]).toMatchObject({ continuity: 'smooth',incoming: { x: 2/3,mode: 'free' },outgoing: { x: 1/3,y: 0.5,mode: 'free' },segment: { kind: 'bezier' } })
  expect((read.keyframes[1]!.incoming as { y: number }).y).toBeCloseTo(0.5,14)
  expect(read.keyframes[0]).toMatchObject({ segment: { kind: 'bezier' } })
  expect(compositionState.history.past).toHaveLength(depth+1)
  const authored = JSON.stringify(compositionState.document)
  for (const invalid of [{ incoming: { x: -1,y: 0 } },{ incoming: undefined,outgoing: undefined,continuity: undefined }]) {
    expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...edit,...invalid }] })).toThrow()
    expect(JSON.stringify(compositionState.document)).toBe(authored)
    expect(compositionState.history.past).toHaveLength(depth+1)
  }
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('retimes keys with tangent preservation, sorted readback, collision refusal and exact Undo', () => {
  const tangents = { incoming: { x: 0.7,y: -1,mode: 'free' },outgoing: { x: 0.3,y: 2,mode: 'free' },continuity: 'broken' }
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1,tangents },{ tick: 2000,value: 2 }] } }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,animation: { x: curve } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const edit = { op: 'move_keyframe',clipId: clip.id,property: 'x',tick: 1000,newTick: 2500 }
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...edit,newTick: 2000 }] })).toThrow('Another key')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [edit] })
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' }) as { keyframes: Record<string,unknown>[] }
  expect(read.keyframes.map(key => key.tick)).toEqual([0,2000,2500])
  expect(read.keyframes[2]).toMatchObject({ value: 1,...tangents })
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it.each([
  { envelope: 'audio',property: 'gain' },
  { envelope: 'audio',property: 'pan' },
  { envelope: 'mask',property: 'x',maskId: 'mask' },
  { envelope: 'mask',property: 'rotationDegrees',maskId: 'mask' },
])('qualifies scalar MCP key authoring and readback on $envelope/$property', subject => {
  compositionState.document = { ...compositionState.document,sources: { media: { id: 'media',kind: 'video',durationTicks: 2_000_000,width: 1280,height: 720,hasAudio: true } } }
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0.2 },{ tick: 1000,value: 0.8 },{ tick: 2000,value: 0.4 }] } }
  const constant = { mode: 'constant',value: 0.5 }
  const mask = { id: 'mask',shape: 'rectangle',x: curve,y: constant,width: constant,height: constant,rotationDegrees: curve,feather: 0,inverted: false }
  const video = { id: 'video',kind: 'video',sourceId: 'media',timelineStartTicks: 3_000_000,sourceInTicks: 0,sourceOutTicks: 2_000_000,transform: { x: 0,y: 0,width: 1280,height: 720,fit: 'contain' },opacity: 1,sourceAudioEnabled: true,audioGain: 1,audioAnimation: { gain: curve,pan: curve },masks: [mask] }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track: { id: 'v',kind: 'video',name: 'Video',locked: false,hidden: false,muted: false,clips: [] } },{ op: 'add_layer',trackId: 'v',clip: video }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const request = { clipId: 'video',...subject }
  const read = () => executeAgentTool('agent-a','get_keyframes',request) as { keyframes: Record<string,unknown>[] }
  const apply = (operation: Record<string,unknown>) => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...request,...operation }] })
  expect(read().keyframes[1]).toMatchObject({ tick: 1000,t_local_us: 1_000_000,t_us: 4_000_000,preset_id: 'linear' })
  apply({ op: 'set_keyframe_tangent_mode',ticks: [0,1000,2000],mode: 'auto' })
  expect(read().keyframes[1]).toMatchObject({ incoming: { y: 1,mode: 'auto' },outgoing: { y: 0,mode: 'auto' } })
  apply({ op: 'edit_keyframe_tangents',tick: 1000,outgoing: { x: 0.25,y: 0.2 },continuity: 'broken' })
  expect(read().keyframes[1]).toMatchObject({ continuity: 'broken',incoming: { mode: 'free' },outgoing: { x: 0.25,y: 0.2,mode: 'free' } })
  apply({ op: 'move_keyframe',tick: 1000,newTick: 1500 })
  expect(read().keyframes[1]).toMatchObject({ tick: 1500,t_us: 4_500_000,outgoing: { x: 0.25,y: 0.2,mode: 'free' } })
  expect(compositionState.history.past).toHaveLength(depth+3)
  const authored = compositionState.document.tracks[0]!.clips[0]!
  if (authored.kind !== 'video') throw new Error('video required')
  if (subject.envelope === 'audio') expect(authored.masks).toEqual([mask])
  else expect(authored.audioAnimation).toEqual(video.audioAnimation)
  undoComposition(); undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  apply({ op: 'upsert_keyframe',tick: 500,value: 0.6 })
  expect(read().keyframes.map(key => key.tick)).toEqual([0,500,1000,2000])
  apply({ op: 'delete_keyframe',tick: 500 })
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth+2)
  undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('creates absent animation, updates exact keys and removes the last key to its constant value', () => {
  executeAgentTool('agent-a','apply_batch',batch())
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const request = { clipId: clip.id,property: 'x' }
  const apply = (operation: Record<string,unknown>) => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...request,...operation }] })
  const read = () => executeAgentTool('agent-a','get_keyframes',request)
  apply({ op: 'upsert_keyframe',tick: 500_000,value: 10 })
  expect(read()).toMatchObject({ mode: 'keyframes',timeBase: 1_000_000,keyframes: [{ tick: 500_000,value: 10 }] })
  apply({ op: 'upsert_keyframe',tick: 500_000,value: 20 })
  expect(read()).toMatchObject({ keyframes: [{ tick: 500_000,value: 20 }] })
  expect(() => apply({ op: 'delete_keyframe',tick: 123 })).toThrow('Selected keyframe')
  expect(compositionState.history.past).toHaveLength(depth+2)
  apply({ op: 'delete_keyframe',tick: 500_000 })
  expect(read()).toMatchObject({ mode: 'constant',value: 20 })
  expect(compositionState.history.past).toHaveLength(depth+3)
  undoComposition(); undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('creates keys with optional canonical presets and rejects unknown ids atomically', () => {
  executeAgentTool('agent-a','apply_batch',batch())
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const upsert = { op: 'upsert_keyframe',clipId: clip.id,property: 'x',tick: 0,value: 0,preset: 'ease_in_quad' }
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...upsert,preset: 'unknown' }] })).toThrow('Unknown easing preset')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [upsert,{ ...upsert,tick: 1_000_000,value: 1,preset: undefined }] })
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'x' }) as { keyframes: Record<string,unknown>[] }
  expect(read.keyframes[0]).toMatchObject({ preset_id: 'ease_in_quad' })
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('edits time, value, easing and tangents as one MCP transaction in defined order', () => {
  const curve = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 1 },{ tick: 1000,value: 2 }] } }
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'add_track',track },{ op: 'add_layer',trackId: track.id,clip: { ...clip,scaleLinked: true,animation: { scaleX: curve,scaleY: curve } } }] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const edit = { op: 'edit_keyframe',clipId: clip.id,property: 'scaleX',tick: 0,newTick: 250,value: 1.2,preset: 'ease_in_quad',outgoing: { x: 0.25,y: 0.4 },continuity: 'broken' }
  expect(() => executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ ...edit,outgoing: { x: 2,y: 0.4 } }] })).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [edit] })
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,property: 'scaleX' }) as { keyframes: Record<string,unknown>[] }
  expect(read.keyframes[0]).toMatchObject({ tick: 250,value: 1.2,continuity: 'broken',outgoing: { x: 0.25,y: 0.4,mode: 'free' },segment: { p1: [0.25,0.4],p2: [2/3,1/3] } })
  expect(read.keyframes[0]).not.toHaveProperty('preset_id')
  const changed = compositionState.document.tracks[0]!.clips[0]!
  if (changed.kind !== 'text') throw new Error('text required')
  expect(changed.animation!.scaleX).toEqual(changed.animation!.scaleY)
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('authors position mode and path progress atomically with dry run and exact Undo', () => {
  executeAgentTool('agent-a','apply_batch',batch())
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const args = { expectedRevision: agentSession.revision,operations: [
    { op: 'switch_position_mode',clipId: clip.id,positionMode: 'path',nodeId: 'path-node' },
    { op: 'set_path_progress',clipId: clip.id,playheadTicks: 1_000_000,value: 0.75 },
  ] }
  executeAgentTool('agent-a','dry_run',args)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(() => executeAgentTool('agent-a','apply_batch',{ ...args,operations: [...args.operations,{ op: 'set_path_progress',clipId: clip.id,playheadTicks: 3_000_000,value: 1 }] })).toThrow('outside')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  executeAgentTool('agent-a','apply_batch',args)
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { mode: 'path',path: { nodes: [{ id: 'path-node',point: { x: 0,y: 0 } }] },progress: { track: { keyframes: [{ tick: 0,value: 0 },{ tick: 1_000_000,value: 0.75 }] } } } })
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('edits path progress easing and tangents without reshaping geometry', () => {
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [...operations,
    { op: 'switch_position_mode',clipId: clip.id,positionMode: 'path',nodeId: 'node' },
    { op: 'set_path_progress',clipId: clip.id,playheadTicks: 1_000_000,value: 1 },
  ] })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  executeAgentTool('agent-a','apply_batch',{ expectedRevision: agentSession.revision,operations: [
    { op: 'set_keyframe_preset',clipId: clip.id,envelope: 'path',property: 'progress',ticks: [0],preset: 'ease_in_quad' },
    { op: 'set_keyframe_tangent_mode',clipId: clip.id,envelope: 'path',property: 'progress',ticks: [0],mode: 'free' },
    { op: 'edit_keyframe_tangents',clipId: clip.id,envelope: 'path',property: 'progress',tick: 0,outgoing: { x: 0.25,y: -0.1 } },
    { op: 'upsert_keyframe',clipId: clip.id,envelope: 'path',property: 'progress',tick: 500_000,value: 0.3 },
  ] })
  const authored = compositionState.document.tracks[0]!.clips[0] as import('../composition/types').TextClip
  expect(authored.position).toMatchObject({ path: { nodes: [{ id: 'node',point: { x: 0,y: 0 } }] },progress: { track: { keyframes: [{ tick: 0,interpolation: { kind: 'bezier',p1: [1/3,0] },tangents: { outgoing: { x: 0.25,y: -0.1 } } },{ tick: 500_000,value: 0.3 },{ tick: 1_000_000,value: 1 }] } } })
  const read = executeAgentTool('agent-a','get_keyframes',{ clipId: clip.id,envelope: 'path',property: 'progress' }) as { keyframes: { tick: number;value: number }[] }
  expect(read.keyframes.map(key => [key.tick,key.value])).toEqual([[0,0],[500_000,0.3],[1_000_000,1]])
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('reads and atomically authors native RGBA keys on solid layers through agent tools', () => {
  executeAgentTool('solid-agent','apply_batch',{ expectedRevision: agentSession.revision,operations: [
    { op: 'add_track',track: { id: 'solids',kind: 'color',name: 'Solids',locked: false,hidden: false,clips: [] } },
    { op: 'add_layer',trackId: 'solids',clip: { id: 'solid',kind: 'color',timelineStartTicks: 0,durationTicks: 2000000,width: 63,height: 31,color: '#000000ff',transform: { x: 0,y: 0,width: 63,height: 31,fit: 'contain' },opacity: 1 } },
  ] })
  expect(executeAgentTool('solid-agent','get_color_track',{ clipId: 'solid' })).toMatchObject({ mode: 'constant',value: { r: 0,g: 0,b: 0,a: 255 } })
  const before = JSON.stringify(compositionState.document)
  executeAgentTool('solid-agent','apply_batch',{ expectedRevision: agentSession.revision,operations: [{ op: 'set_color_key',clipId: 'solid',playheadTicks: 500000,value: { r: 255,g: 0,b: 0,a: 128 } }] })
  expect(executeAgentTool('solid-agent','get_color_track',{ clipId: 'solid' })).toMatchObject({ mode: 'keyframes',keyframes: [{ tick: 0,value: { r: 0,g: 0,b: 0,a: 255 } },{ tick: 500000,value: { r: 255,g: 0,b: 0,a: 128 } }] })
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('exports every caption lane read-only through the MCP runtime and refuses ordinary titles', async () => {
  const { importSrtToComposition } = await import('../state/composition/timeline.svelte.js')
  const { parseSubtitleDocument } = await import('../subtitles/import.js')
  const title = addCompositionTrack('text')
  importSrtToComposition('1\n00:00:01,000 --> 00:00:03,000\nOne\n\n2\n00:00:02,000 --> 00:00:04,000\nTwo')
  const { toggleCompositionTrackFlag } = await import('../state/composition/timeline.svelte.js')
  const captionTrack = compositionState.document.metadata!.captionTextTracks![0]!
  toggleCompositionTrackFlag(captionTrack, 'hidden')
  toggleCompositionTrackFlag(captionTrack, 'locked')
  const doc = JSON.stringify(compositionState.document)
  const revision = agentSession.revision
  const history = compositionState.history.past.length
  const result = executeAgentTool('agent-a', 'export_captions', {format:'srt'}) as {format:string;cues:number;body:string}
  expect(result).toMatchObject({format:'srt',cues:2})
  expect(parseSubtitleDocument(result.body).map(cue=>cue.text)).toEqual(['One','Two'])
  const lane = compositionState.document.metadata!.captionTextTracks![1]!
  expect(executeAgentTool('agent-a','export_captions',{format:'vtt',track_id:lane})).toMatchObject({format:'vtt',cues:1})
  expect(()=>executeAgentTool('agent-a','export_captions',{format:'srt',track_id:title})).toThrow('субтитров')
  expect(()=>executeAgentTool('agent-a','export_captions',{format:'ass'})).toThrow()
  expect(()=>executeAgentTool('agent-a','export_captions',{})).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(doc)
  expect(agentSession.revision).toBe(revision)
  expect(compositionState.history.past).toHaveLength(history)
})

it('merges caption text and union span as one revision-checked edit and refuses collisions atomically', async () => {
  const { importSrtToComposition, toggleCompositionTrackFlag } = await import('../state/composition/timeline.svelte.js')
  importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOne\n\n2\n00:00:02,000 --> 00:00:03,000\nTwo\n\n3\n00:00:04,000 --> 00:00:05,000\nThree')
  const track = compositionState.document.tracks[0]!
  const [one,two,three] = track.clips
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const merge = (ids: string[], revision = agentSession.revision) => executeAgentTool('agent-a','merge_captions',{expectedRevision:revision,layer_ids:ids})
  expect(()=>merge([one!.id,three!.id])).toThrow('пересекается')
  expect(()=>merge([one!.id])).toThrow()
  expect(()=>merge([one!.id,two!.id],-1)).toThrow('Project changed')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  const result = merge([two!.id,one!.id])
  expect(result).toMatchObject({layer_id:one!.id,t_start_us:1_000_000,t_end_us:3_000_000,content:'One\nTwo',merged:2,removed:[two!.id]})
  expect(compositionState.document.tracks[0]!.clips[0]).toEqual({...one,durationTicks:2_000_000,text:'One\nTwo'})
  expect(compositionState.history.past).toHaveLength(depth+1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(merge([three!.id,two!.id])).toMatchObject({t_start_us:2_000_000,t_end_us:5_000_000,content:'Two\nThree'})
  undoComposition()
  toggleCompositionTrackFlag(track.id,'locked')
  const locked = JSON.stringify(compositionState.document)
  expect(()=>merge([one!.id,two!.id])).toThrow('заблокированы')
  expect(JSON.stringify(compositionState.document)).toBe(locked)
})

it('refuses cross-lane captions and ordinary titles without changing the project', async () => {
  const { importSrtToComposition } = await import('../state/composition/timeline.svelte.js')
  executeAgentTool('agent-a','apply_batch',batch())
  importSrtToComposition('1\n00:00:00,000 --> 00:00:02,000\nOne\n\n2\n00:00:01,000 --> 00:00:03,000\nTwo')
  const lanes = compositionState.document.tracks.filter(track => compositionState.document.metadata!.captionTextTracks!.includes(track.id))
  const ids = lanes.map(track=>track.clips[0]!.id)
  const before = JSON.stringify(compositionState.document)
  const args = (ids:string[])=>({expectedRevision:agentSession.revision,layer_ids:ids})
  expect(()=>executeAgentTool('agent-a','merge_captions',args(ids))).toThrow('одной дорожке')
  expect(()=>executeAgentTool('agent-a','merge_captions',args([ids[0]!,'title']))).toThrow('субтитром')
  expect(()=>executeAgentTool('agent-a','merge_captions',args([ids[0]!,ids[0]!]))).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('restyles only requested caption subjects as one revision-guarded undo step',async()=>{
 const {importSrtToComposition}=await import('../state/composition/timeline.svelte.js')
 importSrtToComposition('1\n00:00:00,000 --> 00:00:03,000\nOne\n\n2\n00:00:01,000 --> 00:00:02,000\nTwo')
 const before=JSON.stringify(compositionState.document),depth=compositionState.history.past.length
 const id=compositionState.document.tracks[1]!.clips[0]!.id
 expect(()=>executeAgentTool('agent-a','restyle_captions',{expectedRevision:-1,font_size_px:99})).toThrow('Project changed')
 const result=executeAgentTool('agent-a','restyle_captions',{expectedRevision:agentSession.revision,layer_ids:[id,id],font_size_px:99})
 expect(result).toMatchObject({captions:2,restyled:1})
 expect(compositionState.history.past).toHaveLength(depth+1)
 undoComposition()
 expect(JSON.stringify(compositionState.document)).toBe(before)
 expect(()=>executeAgentTool('agent-a','restyle_captions',{expectedRevision:agentSession.revision})).toThrow('font_family')
 expect(()=>executeAgentTool('agent-a','restyle_captions',{expectedRevision:agentSession.revision,font_size_px:30,layer_ids:[]})).toThrow()
 expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('exports the requested nested composition in its authored clock without leaking the ordinary document',async()=>{
 const {importSrtToComposition}=await import('../state/composition/timeline.svelte.js')
 const {buildCompositionWireDocument}=await import('../composition/payload.js')
 const {nestedComposition,closeNestedComposition,nestedHistory}=await import('../state/nestedComposition.svelte.js')
 importSrtToComposition('1\n00:00:01,000 --> 00:00:03,000\nNested cue')
 const content=buildCompositionWireDocument(compositionState.document)
 const ordinary=JSON.stringify(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{...content,tracks:[],metadata:{}},instances:[]},child:{content,instances:[]}}}
 const before=JSON.stringify(nestedComposition.document),depth=nestedHistory.past.length,revision=agentSession.revision
 try {
  expect(executeAgentTool('agent-a','export_captions',{format:'srt'})).toMatchObject({cues:0,body:''})
  const result=executeAgentTool('agent-a','export_captions',{format:'vtt',composition_id:'child',track_id:content.tracks[0]!.id}) as {cues:number;body:string}
  expect(result.cues).toBe(1)
  expect(()=>executeAgentTool('agent-a','merge_captions',{expectedRevision:agentSession.revision,layer_ids:['first','second']})).toThrow('nested caption merge')
  expect(result.body).toContain('00:00:01.000 --> 00:00:03.000\nNested cue')
  expect(()=>executeAgentTool('agent-a','export_captions',{format:'srt',composition_id:'missing'})).toThrow('не найдена')
  expect(()=>executeAgentTool('agent-a','export_captions',{format:'srt',track_id:content.tracks[0]!.id})).toThrow('этой композиции')
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(JSON.stringify(compositionState.document)).toBe(ordinary)
  expect(nestedHistory.past).toHaveLength(depth)
  expect(agentSession.revision).toBe(revision)
 } finally {closeNestedComposition()}
 expect(()=>executeAgentTool('agent-a','export_captions',{format:'srt',composition_id:'child'})).toThrow('nested project')
})

it('tracks actual committed history identities through Undo, Redo, branching, and no-op calls',()=>{
 executeAgentTool('a','apply_batch',batch())
 const applied=agentActivity.activities.at(-1)!
 expect(applied.historyIds).toHaveLength(1)
 expect(applied.affected).toEqual(expect.arrayContaining([expect.objectContaining({kind:'Layer',id:'title',label:'Hello'}),expect.objectContaining({kind:'Track',id:'captions',label:'Captions'})]))
 expect(applied.effect).toBe('applied')
 setCompositionRoleFlags('dialogue',{muted:true})
 expect(agentActivity.activities.find(item=>item.id===applied.id)?.effect).toBe('applied')
 undoComposition()
 expect(agentActivity.activities.find(item=>item.id===applied.id)?.effect).toBe('reverted')
 redoComposition()
 expect(agentActivity.activities.find(item=>item.id===applied.id)?.effect).toBe('applied')
 executeAgentTool('a','restyle_captions',{expectedRevision:agentSession.revision,font_size_px:48})
 expect(agentActivity.activities.at(-1)!.historyIds).toEqual([])
 expect(agentActivity.activities.at(-1)!.affected??[]).toEqual([])
 undoComposition()
 executeAgentTool('a','apply_batch',{expectedRevision:agentSession.revision,operations:[{op:'add_track',track:{...track,id:'branch'}}]})
 expect(agentActivity.activities.find(item=>item.id===applied.id)?.effect).toBe('unknown')
 expect(agentActivity.activities.at(-1)!.effect).toBe('applied')
})

it('sets ordinary scalar extrapolation without changing key values and restores it with Undo',()=>{
 executeAgentTool('agent-a','apply_batch',batch())
 const before=JSON.stringify(compositionState.document)
 executeAgentTool('agent-a','apply_batch',{expectedRevision:agentSession.revision,operations:[{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:0,value:0},{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:500000,value:1},{op:'set_keyframe_extrapolation',clipId:'title',property:'opacity',side:'after',extrapolation:'offset'}]})
 expect(executeAgentTool('agent-a','get_keyframes',{clipId:'title',property:'opacity'})).toMatchObject({extrapolation:{before:'hold',after:'offset'}})
 undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('jumps ordinary retained history through actual Undo/Redo and rejects discarded IDs',()=>{
 executeAgentTool('agent-a','apply_batch',batch())
 for(const text of ['One','Two','Three'])executeAgentTool('agent-a','apply_batch',{expectedRevision:agentSession.revision,operations:[{op:'update_layer',clipId:'title',patch:{text}}]})
 const history=executeAgentTool('agent-a','get_history',{}) as {entries:{id:string}[]}
 const first=history.entries.at(-3)!.id,last=history.entries.at(-1)!.id
 executeAgentTool('agent-a','jump_history',{expectedRevision:agentSession.revision,entryId:first})
 expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({text:'One'})
 executeAgentTool('agent-a','jump_history',{expectedRevision:agentSession.revision,entryId:last})
 expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({text:'Three'})
 executeAgentTool('agent-a','jump_history',{expectedRevision:agentSession.revision,entryId:first,side:'before'})
 executeAgentTool('agent-a','apply_batch',{expectedRevision:agentSession.revision,operations:[{op:'update_layer',clipId:'title',patch:{text:'Branch'}}]})
 expect(()=>executeAgentTool('agent-a','jump_history',{expectedRevision:agentSession.revision,entryId:last})).toThrow('no longer')
})
