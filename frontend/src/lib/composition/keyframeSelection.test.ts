import { expect, it } from 'vitest'
import { moveSelectedKeyframeTracks, scaleSelectedKeyframeTracks, type SelectedKeyframeTrack } from './keyframeSelection'

const group = (id: string, durationTicks: number, ticks: number[], selectedTicks: number[]): SelectedKeyframeTrack => ({ id, durationTicks, selectedTicks, value: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: ticks.map((tick, value) => ({ tick, value })) } } })
const grid = { numerator: 30, denominator: 1 }
it('snaps once, preserves group spacing and replaces stationary neighbours without mutating inputs', () => {
  const groups = [group('x', 1_000_000, [100_000, 200_000, 300_000], [100_000, 200_000]), group('gain', 2_000_000, [400_000], [400_000])]
  const before = JSON.stringify(groups)
  const result = moveSelectedKeyframeTracks(groups, 100_005, grid)
  expect(result.delta).toBe(100_000)
  expect(result.groups.map((group) => group.selectedTicks)).toEqual([[200_000, 300_000], [500_000]])
  expect(result.groups[0]!.value).toMatchObject({ track: { keyframes: [{ tick: 200_000, value: 0 }, { tick: 300_000, value: 1 }] } })
  expect(JSON.stringify(groups)).toBe(before)
})
it('clamps the whole group at the tightest layer wall and refuses stale selection atomically', () => {
  const groups = [group('a', 500_000, [100_000], [100_000]), group('b', 900_000, [850_000], [850_000])]
  expect(moveSelectedKeyframeTracks(groups, 300_000, grid).delta).toBe(50_000)
  expect(moveSelectedKeyframeTracks(groups, -300_000, grid).delta).toBe(-100_000)
  expect(() => moveSelectedKeyframeTracks([group('missing', 1_000_000, [0], [1])], 0, grid)).toThrow('no longer')
})

it('commits a selected-key move or delete once and restores collisions with one Undo', async () => {
  const { compositionState, resetCompositionForTests, moveCompositionAutomationKeyframes, scaleCompositionKeyframeSelection, deleteCompositionAutomationKeyframes, undoComposition } = await import('../state/composition.svelte')
  const { createComposition, addTrack, addClip } = await import('./commands')
  resetCompositionForTests()
  try {
    let document = addTrack(createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' }), { id: 'text', kind: 'text', name: 'Text', locked: false, hidden: false, clips: [] })
    document = addClip(document, 'text', { id: 'clip', kind: 'text', text: 'Test', timelineStartTicks: 0, durationTicks: 1_000_000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, animation: { x: group('x', 1_000_000, [100_000, 200_000, 300_000], []).value } })
    compositionState.document = document
    const depth = compositionState.history.past.length
    expect(moveCompositionAutomationKeyframes({ kind: 'visual', clipId: 'clip' }, 'x', [100_000, 200_000], 100_000)).toEqual([200_000, 300_000])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    expect(scaleCompositionKeyframeSelection([{ target: { kind: 'visual', clipId: 'clip' }, property: 'x', ticks: [100_000, 200_000] }], 100_000, 2)[0]!.ticks).toEqual([100_000, 300_000])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    deleteCompositionAutomationKeyframes({ kind: 'visual', clipId: 'clip' }, 'x', [100_000, 200_000])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    expect(() => deleteCompositionAutomationKeyframes({ kind: 'visual', clipId: 'clip' }, 'x', [100_000, 999])).toThrow('no longer')
    expect(compositionState.history.past).toHaveLength(depth)
  } finally { resetCompositionForTests() }
})

it('moves keys across clips and properties as one transaction and refuses a locked participant', async () => {
  const { compositionState, resetCompositionForTests, moveCompositionKeyframeSelection, deleteCompositionKeyframeSelection, selectCompositionClip, selectCompositionKeyframe, undoComposition } = await import('../state/composition.svelte')
  const { createComposition, addTrack, addClip, setClipLocked } = await import('./commands')
  resetCompositionForTests()
  try {
    let document = addTrack(createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' }), { id: 'text', kind: 'text', name: 'Text', locked: false, hidden: false, clips: [] })
    for (const [id, start] of [['a', 0], ['b', 2_000_000]] as const) document = addClip(document, 'text', { id, kind: 'text', text: id, timelineStartTicks: start, durationTicks: 1_000_000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, animation: { x: group('x', 1_000_000, [100_000], []).value, y: group('y', 1_000_000, [950_000], []).value } })
    compositionState.document = document
    const subjects = [{ target: { kind: 'visual' as const, clipId: 'a' }, property: 'x' as const, ticks: [100_000] }, { target: { kind: 'visual' as const, clipId: 'b' }, property: 'y' as const, ticks: [950_000] }]
    const depth = compositionState.history.past.length
    expect(moveCompositionKeyframeSelection(subjects, 200_000).map((subject) => subject.ticks)).toEqual([[150_000], [1_000_000]])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    deleteCompositionKeyframeSelection(subjects)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    expect(() => deleteCompositionKeyframeSelection([subjects[0]!, { ...subjects[1]!, ticks: [12] }])).toThrow('no longer')
    expect(compositionState.document).toEqual(document)
    selectCompositionClip('text', 'a', { single: true })
    selectCompositionKeyframe(subjects[0]!.target, subjects[0]!.property, subjects[0]!.ticks[0]!)
    selectCompositionKeyframe(subjects[1]!.target, subjects[1]!.property, subjects[1]!.ticks[0]!, true)
    expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
    const { executeCompositionCommand, compositionCommandState } = await import('./commandRegistry')
    expect(compositionCommandState('composition.nudgeNext').enabled).toBe(true)
    executeCompositionCommand('composition.nudgeNext')
    expect(compositionState.ui.selectedKeyframes[0]!.ticks[0]).toBe(133_333)
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(0)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    selectCompositionKeyframe(subjects[0]!.target, subjects[0]!.property, subjects[0]!.ticks[0]!)
    selectCompositionKeyframe(subjects[1]!.target, subjects[1]!.property, subjects[1]!.ticks[0]!, true)
    expect(compositionCommandState('composition.delete').enabled).toBe(true)
    executeCompositionCommand('composition.delete')
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(compositionState.ui.selectedKeyframes).toEqual([])
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    compositionState.document = setClipLocked(document, 'b', true)
    const lockedBefore = JSON.stringify(compositionState.document)
    expect(() => moveCompositionKeyframeSelection(subjects, 200_000)).toThrow(/locked|заблокирован/i)
    expect(() => deleteCompositionKeyframeSelection(subjects)).toThrow(/locked|заблокирован/i)
    expect(JSON.stringify(compositionState.document)).toBe(lockedBefore)
    expect(compositionState.history.past).toHaveLength(depth)
    expect(() => moveCompositionKeyframeSelection([subjects[0]!, subjects[0]!], 0)).toThrow('subjects')
  } finally { resetCompositionForTests() }
})

it('scales after anchoring in composition time, preserves order and caps the whole group at layer walls', () => {
  const groups = [{ ...group('a', 2_000_000, [100_000, 400_000, 700_000], [100_000, 400_000, 700_000]), timelineStartTicks: 1_000_000 }, { ...group('b', 2_000_000, [700_000], [700_000]), timelineStartTicks: 1_000_000 }]
  const before = JSON.stringify(groups)
  const doubled = scaleSelectedKeyframeTracks(groups, 1_100_000, 2, grid)
  expect(doubled.factor).toBe(2)
  expect(doubled.groups.map((group) => group.selectedTicks)).toEqual([[100_000, 700_000, 1_300_000], [1_300_000]])
  const floored = scaleSelectedKeyframeTracks(groups, 1_100_000, -1, grid)
  expect(floored.factor).toBeCloseTo(33_333 / 300_000)
  expect(floored.groups[0]!.selectedTicks).toEqual([100_000, 133_333, 166_667])
  expect(scaleSelectedKeyframeTracks(groups, 1_100_000, 100, grid).factor).toBeCloseTo(1_900_000 / 600_000)
  expect(scaleSelectedKeyframeTracks(groups, 1_700_000, 2, grid).groups[0]!.selectedTicks).toEqual([0, 366_667, 700_000])
  expect(JSON.stringify(groups)).toBe(before)
  expect(scaleSelectedKeyframeTracks([group('one', 1_000_000, [100_000], [100_000])], 100_000, 2, grid).factor).toBe(1)
})


it('recovers authored keys outside the clip walls and leaves impossible groups unchanged', () => {
  const before = [group('negative', 1_000_000, [-200_000, 300_000], [-200_000, 300_000])]
  const moved = moveSelectedKeyframeTracks(before, 100_000, grid)
  expect(moved.delta).toBe(200_000)
  expect(moved.groups[0]!.selectedTicks).toEqual([0, 500_000])
  const trailing = moveSelectedKeyframeTracks([group('trailing', 1_000_000, [1_200_000], [1_200_000])], 100_000, grid)
  expect(trailing.delta).toBe(-200_000)
  expect(trailing.groups[0]!.selectedTicks).toEqual([1_000_000])
  const wide = [group('wide', 1_000_000, [-200_000, 1_200_000], [-200_000, 1_200_000])]
  const unchanged = moveSelectedKeyframeTracks(wide, 100_000, grid)
  expect(unchanged.delta).toBe(0)
  expect(unchanged.groups).toEqual(wide)
  expect(scaleSelectedKeyframeTracks(wide, 500_000, 0.5, grid).factor).toBe(0.5)
  expect(scaleSelectedKeyframeTracks(wide, 0, 0.5, grid).factor).toBeCloseTo(33_333 / 1_400_000)
  expect(before[0]!.selectedTicks).toEqual([-200_000, 300_000])
})

it('re-solves Auto after a selected key moves between unequal spans', () => {
  const base = group('curve', 3_000_000, [0, 1_000_000, 2_000_000], [1_000_000])
  if (base.value.mode !== 'keyframes') throw new Error('track required')
  const entry: SelectedKeyframeTrack = { ...base, value: { ...base.value, track: { ...base.value.track, interpolation: { kind: 'bezier', p1: [1/3,1/3], p2: [2/3,2/3] }, keyframes: base.value.track.keyframes.map(key => ({ ...key, tangents: { incoming: { x: 2/3, y: 2/3, mode: 'auto' as const }, outgoing: { x: 1/3, y: 1/3, mode: 'auto' as const }, continuity: 'smooth' as const } })) } } }
  const before = JSON.stringify(entry)
  const result = moveSelectedKeyframeTracks([entry], -500_000, grid).groups[0]!.value
  if (result.mode !== 'keyframes') throw new Error('track required')
  expect(result.track.keyframes[1]!.tick).toBe(500_000)
  expect(result.track.keyframes[1]!.tangents!.incoming.y).toBeCloseTo(5/6, 12)
  expect(result.track.keyframes[1]!.tangents!.outgoing.y).toBe(0.5)
  expect(JSON.stringify(entry)).toBe(before)
})

it('writes linked tangent metadata as one history entry and rejects locked or stale edits', async () => {
  const { compositionState, resetCompositionForTests, setCompositionAutomationTangents, undoComposition } = await import('../state/composition.svelte')
  const { createComposition, addTrack, addClip, setClipLocked } = await import('./commands')
  resetCompositionForTests()
  try {
    const curve = { mode: 'keyframes' as const, track: { timeBase: 1_000_000, interpolation: { kind: 'bezier' as const, p1: [0.25,0.1] as const, p2: [0.25,1] as const }, keyframes: [{ tick: 0, value: 1 }, { tick: 1_000_000, value: 2 }] } }
    let document = addTrack(createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' }), { id: 'text', kind: 'text', name: 'Text', locked: false, hidden: false, clips: [] })
    document = addClip(document, 'text', { id: 'clip', kind: 'text', text: 'Test', timelineStartTicks: 0, durationTicks: 2_000_000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, scaleLinked: true, animation: { scaleX: curve, scaleY: structuredClone(curve) } })
    compositionState.document = document
    const depth = compositionState.history.past.length
    setCompositionAutomationTangents({ kind: 'visual', clipId: 'clip' }, 'scaleX', [0], { continuity: 'smooth' }, true)
    const clip = compositionState.document.tracks[0]!.clips[0]!
    if (clip.kind !== 'text') throw new Error('text expected')
    expect(clip.animation!.scaleX).toEqual(clip.animation!.scaleY)
    expect(clip.scaleLinked).toBe(true)
    expect(clip.animation!.scaleX).toMatchObject({ track: { keyframes: [{ tangents: { continuity: 'smooth' } }, {}] } })
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(compositionState.document).toEqual(document)
    expect(() => setCompositionAutomationTangents({ kind: 'visual', clipId: 'clip' }, 'scaleX', [999], { continuity: 'broken' }, true)).toThrow('no longer')
    expect(compositionState.history.past).toHaveLength(depth)
    const locked = setClipLocked(document, 'clip', true)
    compositionState.document = locked
    expect(() => setCompositionAutomationTangents({ kind: 'visual', clipId: 'clip' }, 'scaleX', [0], { continuity: 'smooth' }, true)).toThrow()
    expect(compositionState.document).toEqual(locked)
    expect(compositionState.history.past).toHaveLength(depth)
  } finally { resetCompositionForTests() }
})

it('applies Auto across tracks atomically and refuses a locked participant', async () => {
  const { compositionState, resetCompositionForTests, setCompositionKeyframeSelectionTangentMode, undoComposition } = await import('../state/composition.svelte')
  const { createComposition, addTrack, addClip, setClipLocked } = await import('./commands')
  resetCompositionForTests()
  try {
    let document = createComposition({ width: 1280,height: 720,fps: 30,backgroundColor: '#000000' })
    for (const id of ['a','b']) {
      document = addTrack(document,{ id,kind: 'text',name: id,locked: false,hidden: false,clips: [] })
      document = addClip(document,id,{ id: `${id}-clip`,kind: 'text',text: id,timelineStartTicks: 0,durationTicks: 3_000_000,x: 0,y: 0,opacity: 1,style: { fontSizePx: 48,color: '#FFFFFFFF',align: 'center' },animation: { x: group(id,3_000_000,[0,1_000_000,2_000_000],[]).value } })
    }
    compositionState.document = document
    const depth = compositionState.history.past.length
    const subjects = ['a','b'].map(id => ({ target: { kind: 'visual' as const,clipId: `${id}-clip` },property: 'x' as const,ticks: [1_000_000] }))
    setCompositionKeyframeSelectionTangentMode('auto',subjects)
    expect(compositionState.history.past.length).toBe(depth+1)
    for (const track of compositionState.document.tracks) {
      const clip = track.clips[0]!
      if (clip.kind !== 'text') throw new Error('text required')
      expect(clip.animation!.x).toMatchObject({ track: { keyframes: [{ interpolation: { kind: 'bezier' } },{ tangents: { incoming: { mode: 'auto' },outgoing: { mode: 'auto' },continuity: 'smooth' } },{}] } })
    }
    undoComposition()
    expect(compositionState.document).toEqual(document)
    const locked = setClipLocked(document,'b-clip',true)
    compositionState.document = locked
    expect(() => setCompositionKeyframeSelectionTangentMode('auto',subjects)).toThrow()
    expect(compositionState.document).toEqual(locked)
    expect(compositionState.history.past.length).toBe(depth)
  } finally { resetCompositionForTests() }
})

it('moves clocked keys on the local grid while enforcing local layer walls', () => {
  const value: SelectedKeyframeTrack = { id: 'clocked', durationTicks: 1000000, selectedTicks: [250, 1250], value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, value: 0 }, { tick: 1250, value: 1 }] } } }
  const before = structuredClone(value)
  const result = moveSelectedKeyframeTracks([value], 100000, grid)
  expect(result.groups[0]!.selectedTicks).toEqual([450, 1450])
  expect(moveSelectedKeyframeTracks([value], -100000, grid).delta).toBe(0)
  expect(result.groups[0]!.value).toMatchObject({ track: { sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 } } })
  expect(value).toEqual(before)
})
