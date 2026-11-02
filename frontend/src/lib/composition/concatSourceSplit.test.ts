import type { ConcatSourceDocumentClock, WireClipPlacement, WireConcatParamKey } from './types'
import { evaluateNativeAgentBatch } from '$lib/agent/nativeBatch'
import { beforeEach, expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import { splitNativeClips } from './nativeClipSplit'
import { splitNativeClipsWithSourcePlans } from './concatSourceSplit'
import { closeNestedComposition, nestedComposition, nestedHistory, splitNestedClips, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
vi.mock('$lib/api', () => ({ planConcatSourceEdit: vi.fn(), evaluateNestedFrame: vi.fn() }))
const clock = { start: 2, duration: 3, sourceStart: 1.5, speed: 1.75, speedCurve: [{ at: 0, speed: 1 }, { at: 1, speed: 3 }] }
const run: readonly WireConcatParamKey[] = [{ at: 0, value: 0.2, ease: [0, 0, 1, 1] }, { at: 1, value: 0.8, ease: [0.42, -0.2, 0.58, 1.2] }]
function placement(raw: ConcatSourceDocumentClock): WireClipPlacement {
  return { timelineStartTick: Math.round(raw.start * 1e6), timelineDurationTick: Math.round(raw.duration * 1e6), sourceInTick: Math.round(raw.sourceStart * 1e6), sourceOutTick: Math.round((raw.sourceStart + raw.duration * (raw.speedCurve ? 2 : raw.speed)) * 1e6), speed: raw.speed,
    concatRetime: { documentClock: raw, points: raw.speedCurve ? raw.speedCurve.map(p => [p.at, p.speed] as const) : [], durationTicks: Math.round(raw.duration * 1e6), rateNum: 25, rateDen: 1 } }
}
function document() {
  const project = JSON.parse(JSON.stringify(fixture.nativeFirstFree[0]!.after))
  const node = project.compositions['new-timeline']
  node.content.canvas.fpsRate = { numerator: 25, denominator: 1 }
  node.content.canvas.fpsMilli = 25000
  node.content.metadata.clipNames = { 'auto-first': 'Camera A' }
  node.content.metadata.mediaVersions = { 'auto-first': { test: 'preserve' } }
  for (const index of [1, 3]) {
    const clip = node.content.tracks[index].clips[0]
    clip.placement = placement(clock)
    clip.concatGainKeys = run
    clip.dspEffects = [{ id: 'concat.bass', concatSourceKeys: { gain: run } }]
  }
  const video = node.content.tracks[1].clips[0]
  video.effects = [{ kind: 'concat_contrast', parameters: { amount: 0 }, concatKeys: run }]
  video.audioFadeInTicks = 300000; video.audioFadeOutTicks = 400000
  video.transform.concatAnimation = { durationTicks: 3000000, keys: { opacity: run }, rest: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotationDegrees: 0, opacity: 1 }, stretch: [1, 1], canvas: [16, 16] }
  node.content.tracks[3].clips[0].fadeInTicks = 300000
  node.content.tracks[3].clips[0].fadeOutTicks = 400000
  node.content.tracks[1].transitions = [{ id: 'out', fromClipId: 'auto-first', kind: 'dissolve', durationTicks: 0, reverse: false, timeline: { startTick: String(4750000n * 254016n), durationTick: String(250000n * 254016n) } }]
  return project as unknown as api.NativeNestedProject
}
function response(input: Parameters<typeof api.planConcatSourceEdit>[0]): api.ConcatSourceEditPlan {
  const head = { start: 2, duration: 1.25, sourceStart: 1.5, speed: 1.75 }
  const tail = { start: 3.25, duration: 1.75, sourceStart: 3.6875, speed: 1.75 }
  const mapped = (side: number): api.ConcatSourceEditTracks => Object.fromEntries((['clip', 'audio', 'video'] as const).map(scope => [scope, Object.fromEntries(Object.entries(input.tracks?.[scope] ?? {}).map(([name, keys]) => [name, keys.map(key => ({ ...key, value: side ? 0.65 : 0.35, derived: true }))]))]))
  return { changed: true, rippleBy: 0, parts: [
    { role: 'head', clock: head, placement: placement(head), tracks: mapped(0), fadeIn: input.fadeIn!, fadeOut: 0 },
    { role: 'tail', clock: tail, placement: placement(tail), tracks: mapped(1), fadeIn: 0, fadeOut: input.fadeOut! }
  ] }
}
beforeEach(() => {
  closeNestedComposition(); vi.resetAllMocks()
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async input => response(input))
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
})
it('applies native clocks and every source key scope to linked AV clips in one immutable split', async () => {
  const input = document(), before = JSON.stringify(input)
  let serial = 0
  const output = await splitNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 3250000, () => `tail-${serial++}`)
  expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(2)
  expect(vi.mocked(api.planConcatSourceEdit).mock.calls[0]![0]).toMatchObject({ clock, fadeIn: 0.3, fadeOut: 0.4, tracks: { clip: { opacity: run, volume: run }, audio: { 'audio/0/concatSourceKeys/gain': run }, video: { 'video/0/concatKeys': run } } })
  const tracks = output.compositions['new-timeline']!.content.tracks
  for (const index of [1, 3]) {
    const clips = tracks[index]!.clips
    expect(clips).toHaveLength(2)
    expect(clips[1]).toMatchObject({ placement: { sourceInTick: 3687500, concatRetime: { documentClock: { start: 3.25, sourceStart: 3.6875, duration: 1.75 }, points: [] } }, concatGainKeys: [{ value: 0.65 }, { value: 0.65 }], dspEffects: [{ concatSourceKeys: { gain: [{ value: 0.65 }, { value: 0.65 }] } }] })
    expect(clips[1]).not.toHaveProperty('placement.sourceTimelineOffsetTick')
    expect(clips[0]).toMatchObject({ placement: { timelineDurationTick: 1250000, concatRetime: { durationTicks: 1250000 } } })
  }
  expect(tracks[1]!.clips[1]).toMatchObject({ transform: { concatAnimation: { durationTicks: 1750000, keys: { opacity: [{ value: 0.65 }, { value: 0.65 }] } } }, effects: [{ concatKeys: [{ value: 0.65 }, { value: 0.65 }] }], audioFadeInTicks: 0, audioFadeOutTicks: 400000 })
  expect(tracks[1]).toMatchObject({ transitions: [{ fromClipId: 'tail-0' }] })
  expect(output.compositions['new-timeline']!.content.metadata).toMatchObject({ clipNames: { 'tail-0': 'Camera A' }, clipLinks: { 'auto-first-link': ['auto-first', 'tail-0', expect.any(String), 'tail-1'] } })
  expect(JSON.stringify(input)).toBe(before)
  expect(() => splitNativeClips(input, 'new-timeline', ['auto-first'], 3250000, () => 'unsafe')).toThrow('нативного плана')
})
it('keeps the complete transaction unchanged when a linked plan fails or the source command is a no-op', async () => {
  const input = document(), before = JSON.stringify(input), mint = vi.fn(() => 'tail')
  vi.mocked(api.planConcatSourceEdit).mockImplementationOnce(async input => response(input)).mockRejectedValueOnce(new Error('source refusal'))
  await expect(splitNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 3250000, mint)).rejects.toThrow('source refusal')
  expect(mint).not.toHaveBeenCalled(); expect(JSON.stringify(input)).toBe(before)
  vi.mocked(api.planConcatSourceEdit).mockResolvedValue({ changed: false, rippleBy: 0, parts: [] })
  expect(await splitNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 2001000, mint)).toBe(input)
  expect(mint).not.toHaveBeenCalled()
})
it('records one history entry and invalidates outputs only after native project validation', async () => {
  const input = document(); nestedComposition.document = input; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.renderedOutputs = { 'new-timeline': 'old-render' }
  await splitNestedClips('new-timeline', ['auto-first'], 3250000)
  const after = JSON.parse(JSON.stringify(nestedComposition.document))
  expect(nestedHistory.past).toHaveLength(1); expect(nestedComposition.renderedOutputs).toEqual({})
  undoNestedComposition(); expect(nestedComposition.document).toEqual(input); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'old-render' })
  redoNestedComposition(); expect(nestedComposition.document).toEqual(after)
  closeNestedComposition(); nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'old-render' }
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('retained clock admission closed'))
  await splitNestedClips('new-timeline', ['auto-first'], 3250000)
  expect(nestedComposition.document).toEqual(input); expect(nestedHistory.past).toHaveLength(0); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'old-render' })
})
it('discards a plan arriving after the editor session closes', async () => {
  nestedComposition.document = document()
  let resolve!: (plan: api.ConcatSourceEditPlan) => void
  let captured!: Parameters<typeof api.planConcatSourceEdit>[0]
  vi.mocked(api.planConcatSourceEdit).mockImplementationOnce(input => { captured = input; return new Promise(done => { resolve = done }) })
  const pending = splitNestedClips('new-timeline', ['auto-first'], 3250000)
  await vi.waitFor(() => expect(resolve).toBeTypeOf('function'))
  closeNestedComposition(); resolve(response(captured)); await pending
  expect(nestedComposition.document).toBeNull(); expect(nestedHistory.past).toHaveLength(0)
})

it('refuses stale frame rate and partial source windows before requesting plans', async () => {
  for (const mode of ['rate', 'offset', 'head', 'clock', 'stale', 'points'] as const) {
    const input = JSON.parse(JSON.stringify(document()))
    const p = input.compositions['new-timeline'].content.tracks[1].clips[0].placement
    if (mode === 'rate') p.concatRetime.rateNum = 24
    if (mode === 'offset') p.sourceTimelineOffsetTick = 1
    if (mode === 'head') p.sourceHeadTimelineTick = 1
    if (mode === 'clock') p.concatRetime.documentClock = undefined
    if (mode === 'stale') p.timelineStartTick += 1
    if (mode === 'points') p.concatRetime.points[0][1] += 1
    await expect(splitNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 3250000, () => 'tail')).rejects.toThrow()
  }
  expect(api.planConcatSourceEdit).not.toHaveBeenCalled()
})

it('uses the same native source split in agent batches, with deterministic target IDs and atomic failure', async () => {
  const input = document(), before = JSON.stringify(input)
  const output = await evaluateNativeAgentBatch(input, 'new-timeline', [{ op: 'split_layer', clipId: 'auto-first', atTicks: 3250000, rightClipId: 'agent-tail' }])
  const clips = output.compositions['new-timeline']!.content.tracks[1]!.clips
  expect(clips[1]).toMatchObject({ id: 'agent-tail', placement: { sourceInTick: 3687500, concatRetime: { documentClock: { start: 3.25, duration: 1.75 } } } })
  expect(output.compositions['new-timeline']!.content.tracks[3]!.clips).toHaveLength(2)
  vi.mocked(api.planConcatSourceEdit).mockRejectedValueOnce(new Error('native refusal'))
  await expect(evaluateNativeAgentBatch(input, 'new-timeline', [{ op: 'split_layer', clipId: 'auto-first', atTicks: 3250000, rightClipId: 'agent-tail' }])).rejects.toThrow('native refusal')
  expect(JSON.stringify(input)).toBe(before)
})

import { preflightNativeClipTrim, trimNativeClipsWithSourcePlans } from './concatSourceTrim'
import { trimNativeClipEnds, trimNativeClipStarts } from './nativeClipTrim'
import { trimNestedClipStarts, trimNestedClipEnds } from '$lib/state/nestedComposition.svelte'
function trimResponse(input: Parameters<typeof api.planConcatSourceEdit>[0], raw: ConcatSourceDocumentClock): api.ConcatSourceEditPlan {
  return { changed: true, rippleBy: 0, parts: [{ ...response(input).parts[0]!, role: 'clip', clock: raw, placement: placement(raw), fadeIn: input.fadeIn!, fadeOut: input.fadeOut! }] }
}
it('trims source heads through Rust plans, preserving the curve and applying source/key windows once', async () => {
  const input = document(), before = JSON.stringify(input), raw = { ...clock, start: 2.5, duration: 2.5, sourceStart: 2.375 }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, raw))
  const output = await trimNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 500000, 'start')
  expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(2)
  expect(vi.mocked(api.planConcatSourceEdit).mock.calls[0]![0].command).toEqual({ kind: 'trimStart', delta: 0.5, floor: 0, ripple: false })
  for (const index of [1, 3]) {
    const clip = output.compositions['new-timeline']!.content.tracks[index]!.clips[0]!
    expect(clip).toMatchObject({ placement: { timelineStartTick: 2500000, sourceInTick: 2375000, timelineDurationTick: 2500000, concatRetime: { documentClock: raw, points: [[0, 1], [1, 3]] } }, concatGainKeys: [{ value: 0.35 }, { value: 0.35 }] })
    expect(clip).not.toHaveProperty('placement.sourceTimelineOffsetTick')
  }
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ audioFadeInTicks: 300000, audioFadeOutTicks: 400000, transform: { concatAnimation: { durationTicks: 2500000, keys: { opacity: [{ value: 0.35 }, { value: 0.35 }] } } } })
  expect(JSON.stringify(input)).toBe(before)
  expect(() => trimNativeClipStarts(input, 'new-timeline', ['auto-first'], 500000)).toThrow('нативного плана')
  expect(() => trimNativeClipEnds(input, 'new-timeline', ['auto-first'], -500000)).toThrow('нативного плана')
})
it('moves transitions by the applied clamped boundary, using exact neighboring source bounds', async () => {
  for (const edge of ['start', 'end'] as const) {
    const input = JSON.parse(JSON.stringify(document())), node = input.compositions['new-timeline']
    for (const index of [1, 3]) {
      const neighbour = JSON.parse(JSON.stringify(node.content.tracks[index].clips[0]))
      neighbour.id = `neighbour-${index}`
      neighbour.placement = placement(edge === 'start' ? { start: 0.5, duration: 1, sourceStart: 0, speed: 1 } : { start: 5.5, duration: 1, sourceStart: 0, speed: 1 })
      node.content.tracks[index].clips.push(neighbour)
    }
    if (edge === 'start') node.content.tracks[1].transitions = [{ id: 'in', toClipId: 'auto-first', kind: 'dissolve', durationTicks: 0, reverse: false, timeline: { startTick: String(2000000n * 254016n), durationTick: String(250000n * 254016n) } }]
    const raw = edge === 'start' ? { ...clock, start: 1.5, duration: 3.5, sourceStart: 0.625 } : { ...clock, duration: 3.5 }
    vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, raw))
    const output = await trimNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], edge === 'start' ? -1000000 : 1000000, edge)
    expect(vi.mocked(api.planConcatSourceEdit).mock.lastCall![0].command).toEqual(edge === 'start' ? { kind: 'trimStart', delta: -1, floor: 1.5, ripple: false } : { kind: 'trimEnd', delta: 1, ceiling: 5.5, ripple: false })
    const track = output.compositions['new-timeline']!.content.tracks[1]!
    expect(track).toMatchObject({ transitions: [{ timeline: { startTick: String((edge === 'start' ? 1500000n : 5250000n) * 254016n) } }] })
    expect(track.clips[1]).toEqual(node.content.tracks[1].clips[1])
  }
})
it('retains the full project for source trim no-ops and failed linked requests', async () => {
  const input = document(), before = JSON.stringify(input)
  vi.mocked(api.planConcatSourceEdit).mockResolvedValue({ changed: false, rippleBy: 0, parts: [] })
  expect(await trimNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 1, 'end')).toBe(input)
  vi.mocked(api.planConcatSourceEdit).mockImplementationOnce(async request => trimResponse(request, { ...clock, duration: 2 })).mockRejectedValueOnce(new Error('linked trim refused'))
  await expect(trimNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], -1000000, 'end')).rejects.toThrow('linked trim refused')
  expect(JSON.stringify(input)).toBe(before)
})
it('records editor trim as one undoable transaction and keeps outputs on native refusal', async () => {
  const input = document(); nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'movie' }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, { ...clock, start: 2.5, duration: 2.5, sourceStart: 2.375 }))
  await trimNestedClipStarts('new-timeline', ['auto-first'], 500000)
  const after = JSON.parse(JSON.stringify(nestedComposition.document))
  expect(nestedHistory.past).toHaveLength(1); expect(nestedComposition.renderedOutputs).toEqual({})
  undoNestedComposition(); expect(nestedComposition.document).toEqual(input); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'movie' })
  redoNestedComposition(); expect(nestedComposition.document).toEqual(after)
  closeNestedComposition(); nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'movie' }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, { ...clock, duration: 2 }))
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('admission closed'))
  await trimNestedClipEnds('new-timeline', ['auto-first'], -1000000)
  expect(nestedComposition.document).toEqual(input); expect(nestedHistory.past).toHaveLength(0); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'movie' })
})
it('agent range trim feeds the updated source clock into its second edge edit', async () => {
  const input = document(), before = JSON.stringify(input), head = { ...clock, start: 2.5, duration: 2.5, sourceStart: 2.375 }, final = { ...head, duration: 2 }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, request.command.kind === 'trimStart' ? head : final))
  const output = await evaluateNativeAgentBatch(input, 'new-timeline', [{ op: 'trim_layer', clipId: 'auto-first', startTicks: 2500000, endTicks: 4500000 }])
  expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(4)
  expect(vi.mocked(api.planConcatSourceEdit).mock.calls[2]![0]).toMatchObject({ clock: head, command: { kind: 'trimEnd', delta: -0.5 } })
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ placement: { concatRetime: { documentClock: final } } })
  expect(JSON.stringify(input)).toBe(before)
})

it('does not reject a valid source boundary because independent microsecond rounding creates an apparent overlap', async () => {
  const input = JSON.parse(JSON.stringify(document())), node = input.compositions['new-timeline']
  const before = { ...clock, start: 2.00000051, duration: 2.99999951 }
  const raw = { ...before, duration: 3.49999951 }
  for (const index of [1, 3]) {
    const clip = node.content.tracks[index].clips[0]
    clip.placement = placement(before)
    const neighbour = JSON.parse(JSON.stringify(clip)); neighbour.id = `next-${index}`
    neighbour.placement = placement({ start: 5.50000003, duration: 1, sourceStart: 0, speed: 1 })
    node.content.tracks[index].clips.push(neighbour)
  }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => trimResponse(request, raw))
  const output = await trimNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], 500000, 'end')
  expect(vi.mocked(api.planConcatSourceEdit).mock.lastCall![0].command).toMatchObject({ ceiling: 5.50000003 })
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ placement: { concatRetime: { documentClock: raw } } })
})

it('UI preflight permits native source planning without network calls and still rejects stale or locked participants', () => {
  const input = JSON.parse(JSON.stringify(document()))
  expect(() => preflightNativeClipTrim(input, 'new-timeline', ['auto-first'], -1000000, 'end')).not.toThrow()
  expect(() => preflightNativeClipTrim(input, 'new-timeline', ['auto-first'], 1000000, 'start')).not.toThrow()
  expect(api.planConcatSourceEdit).not.toHaveBeenCalled()
  input.compositions['new-timeline'].content.tracks[3].locked = true
  expect(() => preflightNativeClipTrim(input, 'new-timeline', ['auto-first'], 1000000, 'start')).toThrow('заблокирована')
  input.compositions['new-timeline'].content.tracks[3].locked = false
  input.compositions['new-timeline'].content.tracks[1].clips[0].placement.concatRetime.rateNum = 24
  expect(() => preflightNativeClipTrim(input, 'new-timeline', ['auto-first'], 1000000, 'start')).toThrow('Частота кадров')
})

import { setNativeSourceSpeedCurve, validateSourceSpeedPoints } from './concatSourceCurve'
import { setNestedSourceSpeedCurve } from '$lib/state/nestedComposition.svelte'
const fastPoints = [{ at: 0, speed: 3 }, { at: 1, speed: 3 }]
const fastClock = { ...clock, duration: 1.75, speed: 3, speedCurve: fastPoints }
function curveResponse(input: Parameters<typeof api.planConcatSourceEdit>[0]): api.ConcatSourceEditPlan {
  const p = placement(fastClock)
  return { changed: true, rippleBy: 0, parts: [{ role: 'clip', clock: fastClock, placement: { ...p, sourceOutTick: p.sourceInTick + 5250000 }, tracks: input.tracks!, fadeIn: input.fadeIn!, fadeOut: input.fadeOut! }] }
}
it('sets a linked source curve with native duration/coverage and stretches existing normalized keys unchanged', async () => {
  const input = document(), before = JSON.stringify(input)
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => curveResponse(request))
  const output = await setNativeSourceSpeedCurve(input, 'new-timeline', ['auto-first'], fastPoints)
  expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(2)
  expect(vi.mocked(api.planConcatSourceEdit).mock.calls[0]![0]).toMatchObject({ clock, command: { kind: 'setCurve', points: fastPoints } })
  for (const index of [1, 3]) {
    const clip = output.compositions['new-timeline']!.content.tracks[index]!.clips[0]!
    expect(clip).toMatchObject({ placement: { timelineDurationTick: 1750000, sourceInTick: 1500000, sourceOutTick: 6750000, speed: 3, concatRetime: { documentClock: fastClock } }, concatGainKeys: run, dspEffects: [{ concatSourceKeys: { gain: run } }] })
  }
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ transform: { concatAnimation: { durationTicks: 1750000, keys: { opacity: run } } }, effects: [{ concatKeys: run }], audioFadeInTicks: 300000, audioFadeOutTicks: 400000 })
  expect(output.compositions['new-timeline']!.content.metadata).toEqual(input.compositions['new-timeline']!.content.metadata)
  expect(output.compositions['new-timeline']!.content.tracks[1]).toMatchObject({ transitions: (input.compositions['new-timeline']!.content.tracks[1]! as { transitions: unknown }).transitions })
  expect(JSON.stringify(input)).toBe(before)
})
it('clears source curves through explicit null and retains no-op identity or failed linked edits', async () => {
  const input = document(), before = JSON.stringify(input)
  vi.mocked(api.planConcatSourceEdit).mockResolvedValue({ changed: false, rippleBy: 0, parts: [] })
  expect(await setNativeSourceSpeedCurve(input, 'new-timeline', ['auto-first'], null)).toBe(input)
  expect(vi.mocked(api.planConcatSourceEdit).mock.lastCall![0].command).toEqual({ kind: 'setCurve', points: null })
  vi.mocked(api.planConcatSourceEdit).mockImplementationOnce(async request => curveResponse(request)).mockRejectedValueOnce(new Error('linked curve refused'))
  await expect(setNativeSourceSpeedCurve(input, 'new-timeline', ['auto-first'], fastPoints)).rejects.toThrow('linked curve refused')
  expect(JSON.stringify(input)).toBe(before)
  for (const invalid of [[{ at: NaN, speed: 1 }], [{ at: 0, speed: Infinity }], Array.from({ length: 4097 }, () => ({ at: 0, speed: 1 }))]) expect(() => validateSourceSpeedPoints(invalid)).toThrow()
})
it('uses curve planning in agent batches with strict input fields', async () => {
  const input = document()
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => curveResponse(request))
  const output = await evaluateNativeAgentBatch(input, 'new-timeline', [{ op: 'set_speed_curve', clipIds: ['auto-first'], points: fastPoints }])
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ placement: { speed: 3, timelineDurationTick: 1750000 } })
  for (const op of [{ op: 'set_speed_curve', clipIds: ['auto-first'] }, { op: 'set_speed_curve', clipIds: ['auto-first'], points: [{ at: 0, speed: '3' }] }, { op: 'set_speed_curve', clipIds: ['auto-first'], points: fastPoints, ripple: true }]) await expect(evaluateNativeAgentBatch(input, 'new-timeline', [op])).rejects.toThrow()
})
it('commits source curve changes as one editor history entry and keeps refused projects unchanged', async () => {
  const input = document(); nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'old-render' }
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => curveResponse(request))
  await setNestedSourceSpeedCurve('new-timeline', ['auto-first'], fastPoints)
  const after = JSON.parse(JSON.stringify(nestedComposition.document))
  expect(nestedHistory.past).toHaveLength(1); expect(nestedComposition.renderedOutputs).toEqual({})
  undoNestedComposition(); expect(nestedComposition.document).toEqual(input); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'old-render' })
  redoNestedComposition(); expect(nestedComposition.document).toEqual(after)
  closeNestedComposition(); nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'old-render' }
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('admission closed'))
  await setNestedSourceSpeedCurve('new-timeline', ['auto-first'], fastPoints)
  expect(nestedComposition.document).toEqual(input); expect(nestedHistory.past).toHaveLength(0); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'old-render' })
})

import { duplicateNativeClipsWithSourcePlans } from './concatSourceCopy'
import { duplicateNestedClips } from '$lib/state/nestedComposition.svelte'
function movedCopyResponse(input: Parameters<typeof api.planConcatSourceEdit>[0]): api.ConcatSourceEditPlan {
  const raw = { ...input.clock, start: 8.00000049 }, p = placement(raw)
  return { changed: true, rippleBy: 0, parts: [{ role: 'clip', clock: raw, placement: p, tracks: structuredClone(input.tracks!), fadeIn: input.fadeIn!, fadeOut: input.fadeOut! }] }
}
function fractionalDocument(): api.NativeNestedProject {
  const input = JSON.parse(JSON.stringify(document()))
  for (const index of [1, 3]) input.compositions['new-timeline'].content.tracks[index].clips[0].placement = placement({ ...clock, start: 2.00000049 })
  return input
}
it('relocates copied source clocks through Rust plans while preserving fractional phase and original owners', async () => {
  const input = fractionalDocument(), before = JSON.stringify(input); let serial = 0
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => movedCopyResponse(request))
  const output = await duplicateNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], () => `copy-${serial++}`, 8000000)
  expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(2)
  expect(vi.mocked(api.planConcatSourceEdit).mock.calls[0]![0]).toMatchObject({ clock: { start: 2.00000049 }, command: { kind: 'move', startTick: 8000000 } })
  for (const index of [1, 3]) {
    const clips = output.compositions['new-timeline']!.content.tracks[index]!.clips
    expect(clips[0]).toEqual(input.compositions['new-timeline']!.content.tracks[index]!.clips[0])
    expect(clips[1]).toMatchObject({ placement: { timelineStartTick: 8000000, concatRetime: { documentClock: { start: 8.00000049, sourceStart: 1.5, duration: 3 }, points: [[0, 1], [1, 3]] } }, concatGainKeys: run })
    expect(clips[1]!.id).not.toBe(clips[0]!.id)
  }
  expect(JSON.stringify(input)).toBe(before)
})
it('does not commit private source copies when any linked relocation plan fails', async () => {
  const input = fractionalDocument(), before = JSON.stringify(input); let serial = 0
  vi.mocked(api.planConcatSourceEdit).mockImplementationOnce(async request => movedCopyResponse(request)).mockRejectedValueOnce(new Error('relocation refused'))
  await expect(duplicateNativeClipsWithSourcePlans(input, 'new-timeline', ['auto-first'], () => `copy-${serial++}`, 8000000)).rejects.toThrow('relocation refused')
  expect(JSON.stringify(input)).toBe(before)
})
it('routes agent duplicates and editor history through relocated source copy clocks', async () => {
  const input = fractionalDocument()
  vi.mocked(api.planConcatSourceEdit).mockImplementation(async request => movedCopyResponse(request))
  const output = await evaluateNativeAgentBatch(input, 'new-timeline', [{ op: 'duplicate_layer', clipId: 'auto-first', id: 'agent-copy', startTicks: 8000000 }])
  expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[1]).toMatchObject({ id: 'agent-copy', placement: { concatRetime: { documentClock: { start: 8.00000049 } } } })
  nestedComposition.document = input; nestedComposition.renderedOutputs = { 'new-timeline': 'movie' }
  const ids = await duplicateNestedClips('new-timeline', ['auto-first'], 8000000)
  expect(ids).toHaveLength(2); expect(nestedHistory.past).toHaveLength(1); expect(nestedComposition.renderedOutputs).toEqual({})
  undoNestedComposition(); expect(nestedComposition.document).toEqual(input); expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'movie' })
  redoNestedComposition(); expect(nestedComposition.document?.compositions['new-timeline']!.content.tracks[1]!.clips[1]).toMatchObject({ placement: { concatRetime: { documentClock: { start: 8.00000049 } } } })
})
