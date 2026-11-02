import { setShortcutBinding, resetShortcutBindings } from '$lib/state/shortcuts.svelte.js'
// @ts-expect-error Vitest SSR condition exposes server-only mount; use Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, setClipLocked } from '$lib/composition/commands'
import { compositionState, clearCompositionAutomation, setCompositionScaleLinked, applyCompositionVisualAnimation, setCompositionKeyframeSelectionInterpolation, deleteCompositionKeyframeSelection, copyCompositionKeyframeSelection, pasteCompositionKeyframeClipboard, resetCompositionForTests, selectCompositionClip, selectCompositionKeyframe, setCompositionAutomationKeyframe, undoComposition } from '$lib/state/composition.svelte'
import MultiTrackTimeline from './MultiTrackTimeline.svelte'
let target: HTMLDivElement
beforeEach(() => {
  resetCompositionForTests()
  let draft = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const [id, start, property, keyTick] of [['a', 0, 'x', 100_000], ['b', 1_000_000, 'y', 500_000]] as const) {
    draft = addTrack(draft, { id, kind: 'text', name: id, locked: false, hidden: false, clips: [] })
    draft = addClip(draft, id, { id: `${id}-clip`, kind: 'text', text: id, timelineStartTicks: start, durationTicks: 2_000_000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, animation: { [property]: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: keyTick, value: 0 }] } } } })
  }
  compositionState.document = draft
  selectCompositionClip('a', 'a-clip', { single: true })
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 100_000)
  selectCompositionKeyframe({ kind: 'visual', clipId: 'b-clip' }, 'y', 500_000, true)
  target = document.createElement('div')
  globalThis.document.body.append(target)
})
afterEach(() => { target.remove(); resetCompositionForTests() })
it.each(['move', 'scale', 'cancel'] as const)('previews multi-layer keyframe drag without changing the project: %s', async (mode) => {
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamonds = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')]
  expect(diamonds).toHaveLength(2)
  const anchor = diamonds[mode === 'scale' ? 1 : 0]!
  anchor.setPointerCapture = () => {}
  anchor.hasPointerCapture = () => false
  const x = compositionState.ui.zoomPxPerSecond * (mode === 'scale' ? 1.4 : 0.1)
  anchor.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0, altKey: mode === 'scale' }))
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: x }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  const projected = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')].map((diamond) => Number.parseFloat(diamond.style.left))
  if (mode === 'cancel') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  else window.dispatchEvent(new MouseEvent('pointerup', { clientX: x }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  if (mode !== 'cancel') {
    expect(compositionState.history.past).toHaveLength(depth + 1)
    const landed = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')].map((diamond) => Number.parseFloat(diamond.style.left))
    expect(landed).toEqual(projected)
    expect(compositionState.ui.selectedKeyframes.map((subject) => subject.ticks)).toEqual(mode === 'scale' ? [[100_000], [1_900_000]] : [[200_000], [600_000]])
    undoComposition()
  }
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})
it('picks across lanes with Shift and deletes keys without deleting the inspected clip', async () => {
  compositionState.ui.selectedKeyframes = []
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamonds = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')]
  diamonds[0]!.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }))
  diamonds[1]!.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0, shiftKey: true }))
  expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
  expect(compositionState.ui.selectedClipId).toBe('a-clip')
  diamonds[0]!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Delete' }))
  await tick()
  expect(compositionState.document.tracks.reduce((count, track) => count + track.clips.length, 0)).toBe(2)
  expect(target.querySelector('.composition-keyframe-diamond')).toBeNull()
  await unmount(component)
})
it('clears only key selection on sublane background without moving the playhead or inspected clip', async () => {
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const depth = compositionState.history.past.length
  const lane = target.querySelector<HTMLElement>('.composition-keyframe-lane')!
  lane.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }))
  expect(compositionState.ui.selectedKeyframes).toEqual([])
  expect(compositionState.ui.selectedClipId).toBe('a-clip')
  expect(compositionState.transport.playheadTicks).toBe(0)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})
it.each(['keep', 'escape', 'cancel'] as const)('marquees keys across sublanes, shrinks live and restores the press selection: %s', async (mode) => {
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 100_000)
  const previous = JSON.stringify(compositionState.ui.selectedKeyframes)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const scroll = target.querySelector<HTMLElement>('.composition-timeline-scroll')!
  scroll.getBoundingClientRect = () => new DOMRect(0, 0, 800, 300)
  scroll.setPointerCapture = () => {}
  scroll.hasPointerCapture = () => false
  const lanes = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-lane')]
  lanes.forEach((lane, index) => { lane.getBoundingClientRect = () => new DOMRect(0, 100 + index * 120, 800, 24) })
  const diamonds = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')]
  diamonds.forEach((diamond, index) => { diamond.getBoundingClientRect = () => new DOMRect(92 + index * 100, 102 + index * 120, 16, 20) })
  lanes[0]!.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 300, clientY: 118 }))
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: 50, clientY: 235 }))
  await tick()
  expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
  expect(compositionState.ui.selectedClipId).toBe('a-clip')
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: 150, clientY: 235 }))
  expect(compositionState.ui.selectedKeyframes.map((subject) => subject.target.clipId)).toEqual(['b-clip'])
  if (mode === 'escape') {
    lanes[0]!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    lanes[1]!.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0, clientX: 150, clientY: 235 }))
  }
  else window.dispatchEvent(new MouseEvent(mode === 'cancel' ? 'pointercancel' : 'pointerup', { clientX: 150, clientY: 235 }))
  await tick()
  if (mode !== 'keep') expect(JSON.stringify(compositionState.ui.selectedKeyframes)).toBe(previous)
  expect(target.querySelector('.composition-marquee')).toBeNull()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})

it('draws value curves and selects only the expanded diamond inside both marquee axes', async () => {
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 10, 1_100_000)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const row = target.querySelector<HTMLElement>('[data-keyframe-track="a"]')!
  row.querySelector<HTMLButtonElement>('.composition-track-head button')!.click()
  await tick()
  const curve = row.querySelector<SVGPolylineElement>('polyline:not(.composition-keyframe-tail):not(.composition-keyframe-segment-hit)')!
  expect(curve.getAttribute('points')).toContain('50.400,60.000')
  const diamonds = [...row.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')]
  expect(diamonds.map((diamond) => Number.parseFloat(diamond.style.top))).toEqual([110, 10])
  const scroll = target.querySelector<HTMLElement>('.composition-timeline-scroll')!
  scroll.getBoundingClientRect = () => new DOMRect(0, 0, 800, 400)
  scroll.setPointerCapture = () => {}
  scroll.hasPointerCapture = () => false
  const lanes = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-lane')]
  lanes.forEach((lane, index) => { lane.getBoundingClientRect = () => new DOMRect(0, 100 + index * 150, 800, index ? 24 : 120) })
  for (const diamond of target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')) diamond.getBoundingClientRect = () => new DOMRect(Number.parseFloat(diamond.style.left) - 8, diamond.closest<HTMLElement>('.composition-keyframe-lane')!.getBoundingClientRect().top + Number.parseFloat(diamond.style.top) - 10, 16, 20)
  lanes[0]!.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 200, clientY: 150 }))
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: 0, clientY: 100 }))
  await tick()
  expect(compositionState.ui.selectedKeyframes).toEqual([{ target: { kind: 'visual', clipId: 'a-clip' }, property: 'x', ticks: [1_100_000] }])
  window.dispatchEvent(new MouseEvent('pointerup', { clientX: 0, clientY: 100 }))
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})

it('authors parameterized Elastic on selected keys and cancels transient parameter previews', async () => {
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 100_000)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('.composition-keyframe-diamond')!
  const open = async () => { diamond.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); await tick() }
  await open()
  const amplitude = target.querySelector<HTMLInputElement>('input[aria-label="Амплитуда Elastic выбранных ключей"]')!
  amplitude.value = '2'
  amplitude.dispatchEvent(new Event('input', { bubbles: true }))
  const option = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find((item) => item.textContent === 'Elastic Out')!
  option.dispatchEvent(new Event('pointerenter'))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.ui.keyframePreviews[0]?.value).toMatchObject({ track: { keyframes: [{ interpolation: { kind: 'elastic', dir: 'out', amplitude: 2, period: 0.3 } }] } })
  option.click()
  await tick()
  expect(compositionState.history.past.length).toBe(depth + 1)
  const authored = JSON.stringify(compositionState.document)
  await open()
  expect(target.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe('Elastic Out')
  const period = target.querySelector<HTMLInputElement>('input[aria-label="Период Elastic выбранных ключей"]')!
  period.value = '0.17'
  period.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(authored)
  period.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  period.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(authored)
  expect(compositionState.ui.keyframePreviews).toEqual([])
  await open()
  const committedPeriod = target.querySelector<HTMLInputElement>('input[aria-label="Период Elastic выбранных ключей"]')!
  committedPeriod.value = '0.17'
  committedPeriod.dispatchEvent(new Event('input', { bubbles: true }))
  committedPeriod.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.history.past.length).toBe(depth + 2)
  expect(compositionState.document.tracks[0]?.clips[0]).toMatchObject({ animation: { x: { track: { keyframes: [{ interpolation: { amplitude: 2, period: 0.17 } }] } } } })
  undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('edits outgoing Bézier control points with transient previews and exact Undo', async () => {
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 100_000)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('.composition-keyframe-diamond')!
  const open = async () => { diamond.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })); await tick() }
  await open()
  const option = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find((item) => item.textContent === 'Bézier')!
  option.dispatchEvent(new Event('pointerenter'))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.ui.keyframePreviews[0]?.value).toMatchObject({ track: { keyframes: [{ interpolation: { kind: 'bezier', p1: [0.25, 0.1], p2: [0.25, 1] } }] } })
  option.click()
  await tick()
  const authored = JSON.stringify(compositionState.document)
  expect(compositionState.history.past.length).toBe(depth + 1)
  await open()
  expect(target.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe('ease')
  const y = target.querySelector<HTMLInputElement>('input[aria-label="Bézier p1 Y выбранных ключей"]')!
  y.value = '-2'
  y.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(authored)
  y.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  y.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(authored)
  expect(compositionState.ui.keyframePreviews).toEqual([])
  await open()
  const committedY = target.querySelector<HTMLInputElement>('input[aria-label="Bézier p1 Y выбранных ключей"]')!
  committedY.value = '-2'
  committedY.dispatchEvent(new Event('input', { bubbles: true }))
  committedY.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  committedY.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.history.past.length).toBe(depth + 2)
  expect(compositionState.document.tracks[0]?.clips[0]).toMatchObject({ animation: { x: { track: { keyframes: [{ interpolation: { kind: 'bezier', p1: [0.25, -2] } }] } } } })
  const valid = JSON.stringify(compositionState.document)
  const x = target.querySelector<HTMLInputElement>('input[aria-label="Bézier p1 X выбранных ключей"]')!
  x.value = '-0.1'
  x.dispatchEvent(new Event('input', { bubbles: true }))
  x.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(valid)
  undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('applies context easing to the retained multi-lane selection with one Undo', async () => {
  const before = JSON.stringify(compositionState.document)
  const selection = JSON.stringify(compositionState.ui.selectedKeyframes)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 100, clientY: 100 }))
  await tick()
  expect(JSON.stringify(compositionState.ui.selectedKeyframes)).toBe(selection)
  expect(compositionState.transport.playheadTicks).toBe(0)
  const options = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')]
  expect(options.every((option) => option.getAttribute('aria-checked') === 'false')).toBe(true)
  options.find((option) => option.textContent === 'ease_in')!.click()
  await tick()
  expect(target.querySelector('[role="menu"]')).toBeNull()
  expect(compositionState.history.past).toHaveLength(depth + 1)
  for (const track of compositionState.document.tracks) {
    expect(track.clips[0]).toMatchObject({ animation: { [track.id === 'a' ? 'x' : 'y']: { track: { interpolation: 'linear', keyframes: [{ interpolation: 'ease_in' }] } } } })
  }
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})
it('right-clicking an unselected key replaces the key selection and Escape dismisses without editing', async () => {
  selectCompositionKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 100_000)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamonds = [...target.querySelectorAll<HTMLElement>('.composition-keyframe-diamond')]
  diamonds[1]!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  await tick()
  expect(compositionState.ui.selectedKeyframes).toMatchObject([{ target: { clipId: 'b-clip' }, ticks: [500_000] }])
  expect(compositionState.transport.playheadTicks).toBe(1_500_000)
  expect(target.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe('linear')
  diamonds[1]!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }))
  await tick()
  expect(compositionState.ui.selectedKeyframes).toMatchObject([{ target: { clipId: 'b-clip' }, ticks: [500_000] }])
  expect(target.querySelector('[role="menu"]')).toBeNull()
  expect(compositionState.history.past).toHaveLength(depth)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it.each(['refuse', 'recover'] as const)('does not commit a stale projection after invalid drag coordinates: %s', async (mode) => {
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('.composition-keyframe-diamond')!
  const zoom = compositionState.ui.zoomPxPerSecond
  diamond.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0 }))
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: zoom * 0.1 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  compositionState.ui.zoomPxPerSecond = 0
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: zoom * 0.2 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  expect(JSON.stringify(compositionState.document)).toBe(before)
  if (mode === 'recover') compositionState.ui.zoomPxPerSecond = zoom
  window.dispatchEvent(new MouseEvent('pointerup', { clientX: zoom * 0.2 }))
  await tick()
  expect(compositionState.history.past).toHaveLength(depth + (mode === 'recover' ? 1 : 0))
  if (mode === 'recover') {
    expect(compositionState.ui.selectedKeyframes.map((subject) => subject.ticks)).toEqual([[300_000], [700_000]])
    undoComposition()
  }
  expect(JSON.stringify(compositionState.document)).toBe(before)
  compositionState.ui.zoomPxPerSecond = zoom
  await unmount(component)
})

it('previews menu easing across selected lanes without project/history edits and clears on Escape', async () => {
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 10, 1_100_000)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLButtonElement>('.composition-keyframe-row .composition-track-head button')!.click()
  await tick()
  const curve = target.querySelector<SVGPolylineElement>('.composition-keyframe-curve polyline:not(.composition-keyframe-tail):not(.composition-keyframe-segment-hit)')!
  const originalCurve = curve.getAttribute('points')
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  await tick()
  const option = [...target.querySelectorAll<HTMLElement>('[role="menuitemradio"]')].find((item) => item.textContent === 'ease_out')!
  option.dispatchEvent(new MouseEvent('pointerenter'))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  expect(curve.getAttribute('points')).not.toBe(originalCurve)
  for (const preview of compositionState.ui.keyframePreviews) expect(preview.value).toMatchObject({ track: { keyframes: expect.arrayContaining([expect.objectContaining({ interpolation: 'ease_out' })]) } })
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  option.dispatchEvent(new MouseEvent('pointerleave'))
  expect(compositionState.ui.keyframePreviews).toEqual([])
  option.focus()
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  expect(curve.getAttribute('points')).toBe(originalCurve)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})

it('opens outgoing-segment easing from the curve without seeking and commits its owner key', async () => {
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 10, 1_100_000)
  compositionState.ui.selectedKeyframes = []
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLButtonElement>('.composition-keyframe-row .composition-track-head button')!.click()
  await tick()
  const hit = target.querySelector<SVGPolylineElement>('.composition-keyframe-segment-hit')!
  expect(hit.dataset.segmentTick).toBe('100000')
  const playhead = compositionState.transport.playheadTicks
  const depth = compositionState.history.past.length
  hit.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  await tick()
  expect(compositionState.transport.playheadTicks).toBe(playhead)
  expect(compositionState.ui.selectedKeyframes).toMatchObject([{ ticks: [100_000] }])
  const option = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find((item) => item.textContent === 'hold')!
  option.click()
  await tick()
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ animation: { x: { track: { keyframes: [{ interpolation: 'hold' }, { tick: 1_100_000 }] } } } })
  await unmount(component)
})

it('does not clear a newer preview when the old menu is dismissed', async () => {
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }))
  await tick()
  const option = [...target.querySelectorAll<HTMLElement>('[role="menuitemradio"]')].find((item) => item.textContent === 'ease_out')!
  compositionState.ui.keyframePreviews = [{ target: { kind: 'visual', clipId: 'b-clip' }, property: 'opacity', ticks: [], value: { mode: 'constant', value: 0.7 } }]
  option.dispatchEvent(new MouseEvent('pointerenter'))
  expect(compositionState.ui.keyframePreviews).toHaveLength(3)
  const unrelated = compositionState.ui.keyframePreviews[0]!
  compositionState.ui.keyframePreviews = [unrelated, { target: { kind: 'visual', clipId: 'a-clip' }, property: 'x', ticks: [100_000], value: { mode: 'constant', value: 42 } }]
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toMatchObject([{ value: { mode: 'constant', value: 0.7 } }, { value: { mode: 'constant', value: 42 } }])
  await unmount(component)
})

it.each(['escape', 'pointercancel'] as const)('scopes key drag cancellation to its own projections and preserves a newer replacement: %s', async (mode) => {
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  compositionState.ui.keyframePreviews = [{ target: { kind: 'visual', clipId: 'b-clip' }, property: 'opacity', ticks: [], value: { mode: 'constant', value: 0.7 } }]
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0 }))
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: compositionState.ui.zoomPxPerSecond * 0.1 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(3)
  const unrelated = compositionState.ui.keyframePreviews[0]!
  const originalOther = compositionState.ui.keyframePreviews[2]!
  compositionState.ui.keyframePreviews = [unrelated, { target: { kind: 'visual', clipId: 'a-clip' }, property: 'x', ticks: [], value: { mode: 'constant', value: 42 } }, originalOther]
  if (mode === 'escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  else window.dispatchEvent(new MouseEvent('pointercancel'))
  await tick()
  expect(compositionState.ui.keyframePreviews).toMatchObject([{ value: { value: 0.7 } }, { value: { value: 42 } }])
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})

it.each(['ContextMenu', 'F10'])('opens key easing from the keyboard: %s', async (key) => {
  const before = JSON.stringify(compositionState.document)
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key, shiftKey: key === 'F10' }))
  await tick()
  expect(target.querySelector('[role="menu"]')).not.toBeNull()
  expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('navigates menu presets with arrows/Home/End and restores the opener on Escape', async () => {
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('.composition-keyframe-diamond')!
  diamond.focus()
  diamond.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'F10', shiftKey: true }))
  await tick()
  const items = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')]
  expect(document.activeElement).toBe(items[0])
  items[0]!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'ArrowUp' }))
  expect(document.activeElement).toBe(items.at(-1))
  items.at(-1)!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Home' }))
  expect(document.activeElement).toBe(items[0])
  items[0]!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'End' }))
  expect(document.activeElement).toBe(items.at(-1))
  items.at(-1)!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }))
  await tick()
  expect(document.activeElement).toBe(diamond)
  expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
  expect(compositionState.ui.keyframePreviews).toEqual([])
  await unmount(component)
})

it.each([false, true])('dismisses and clears keyboard preview on Tab without blocking traversal: shift=%s', async (shiftKey) => {
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('.composition-keyframe-diamond')!
  diamond.focus()
  diamond.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'F10', shiftKey: true }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'Tab', shiftKey })
  document.activeElement!.dispatchEvent(event)
  await tick()
  expect(event.defaultPrevented).toBe(false)
  expect(target.querySelector('[role="menu"]')).toBeNull()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  await unmount(component)
})

it('copies selected keys with Ctrl+C into the shared slot independently of later edits and Undo', async () => {
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  compositionState.ui.clipboard = { kind: 'layer', clipId: 'a-clip' }
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'c', ctrlKey: true }))
  expect(compositionState.ui.clipboard).toMatchObject({ kind: 'keyframes', groups: [{ parameter: '["visual","x"]', keys: [{ tick: 0, value: 0, interpolation: 'linear' }] }, { parameter: '["visual","y"]', keys: [{ tick: 1_400_000, value: 0, interpolation: 'linear' }] }] })
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  const copied = JSON.stringify(compositionState.ui.clipboard)
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 42, 100_000)
  expect(JSON.stringify(compositionState.ui.clipboard)).toBe(copied)
  undoComposition()
  expect(JSON.stringify(compositionState.ui.clipboard)).toBe(copied)
  await unmount(component)
})

it('pastes copied keys onto selected clips at the playhead with one Undo and selects landed keys', async () => {
  copyCompositionKeyframeSelection()
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  compositionState.transport.playheadTicks = 500_000
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-lane')!.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'v', ctrlKey: true }))
  await tick()
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionState.ui.selectedKeyframes).toMatchObject([{ target: { clipId: 'a-clip' }, property: 'x', ticks: [500_000] }, { target: { clipId: 'a-clip' }, property: 'y', ticks: [1_900_000] }])
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ animation: { x: { track: { keyframes: [{ tick: 100_000 }, { tick: 500_000 }] } }, y: { track: { keyframes: [{ tick: 1_900_000 }] } } } })
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})
it('refuses keyframe paste without a selected target without editing history', () => {
  copyCompositionKeyframeSelection()
  compositionState.ui.selectedClipIds = []
  compositionState.ui.selectedClipId = null
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(() => pasteCompositionKeyframeClipboard()).toThrow('Select a clip')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
})

it.each(['locked'] as const)('refuses the whole key paste if one target is inadmissible: %s', (mode) => {
  copyCompositionKeyframeSelection()
  compositionState.ui.selectedClipIds = ['a-clip', 'b-clip']
  compositionState.transport.playheadTicks = mode === 'locked' ? 1_000_000 : 0
  if (mode === 'locked') compositionState.document = setClipLocked(compositionState.document, 'b-clip', true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(() => pasteCompositionKeyframeClipboard()).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
})

it('honours reassigned copy/paste bindings and leaves the former shortcuts inactive on key lanes', async () => {
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const lane = target.querySelector<HTMLElement>('.composition-keyframe-lane')!
  try {
    setShortcutBinding('composition.copy', 'Mod+Shift+KeyC')
    setShortcutBinding('composition.pasteInsert', null)
    setShortcutBinding('composition.paste', 'Mod+Shift+KeyV')
    lane.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'c', ctrlKey: true }))
    expect(compositionState.ui.clipboard).toBeNull()
    lane.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'c', ctrlKey: true, shiftKey: true }))
    expect(compositionState.ui.clipboard?.kind).toBe('keyframes')
    compositionState.transport.playheadTicks = 500_000
    const depth = compositionState.history.past.length
    lane.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'v', ctrlKey: true }))
    expect(compositionState.history.past).toHaveLength(depth)
    lane.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'v', ctrlKey: true, shiftKey: true }))
    expect(compositionState.history.past).toHaveLength(depth + 1)
  } finally { resetShortcutBindings(); await unmount(component) }
})

it.each(['move', 'scale', 'cancel'] as const)('keeps linked Scale twins through timeline preview and commit: %s', async (mode) => {
  const id = 'a-clip'
  applyCompositionVisualAnimation(id, { x: { mode: 'constant', value: 0 }, scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 100_000, value: 1 }, { tick: 500_000, value: 2 }] } } })
  setCompositionScaleLinked(id, true)
  compositionState.ui.selectedKeyframes = [{ target: { kind: 'visual', clipId: id }, property: 'scaleX', ticks: [100_000, 500_000] }]
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  expect(target.querySelector('[aria-label^="Ключ a-clip scaleY"]')).toBeNull()
  const diamonds = [...target.querySelectorAll<HTMLElement>('[aria-label^="Ключ a-clip scaleX"]')]
  expect(diamonds).toHaveLength(2)
  const anchor = diamonds[mode === 'scale' ? 1 : 0]!
  anchor.setPointerCapture = () => {}
  anchor.hasPointerCapture = () => false
  anchor.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0, altKey: mode === 'scale' }))
  const x = compositionState.ui.zoomPxPerSecond * 0.1
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: x }))
  await tick()
  const previews = compositionState.ui.keyframePreviews
  expect(previews).toHaveLength(2)
  expect(previews.map((entry) => entry.property).sort()).toEqual(['scaleX', 'scaleY'])
  expect(previews[0]!.value).toEqual(previews[1]!.value)
  expect(previews[0]!.ticks).toHaveLength(2)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  if (mode === 'cancel') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  else window.dispatchEvent(new MouseEvent('pointerup', { clientX: x }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  const clip = () => compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
  const landed = clip()
  if (landed.kind === 'audio') throw new Error('visual required')
  expect(landed.scaleLinked).toBe(true)
  expect(landed.animation?.scaleX).toEqual(landed.animation?.scaleY)
  expect(compositionState.ui.selectedKeyframes).toHaveLength(1)
  if (mode !== 'cancel') {
    expect(landed.animation?.scaleX).toEqual(previews[0]!.value)
    expect(compositionState.history.past.length).toBe(depth + 1)
    undoComposition()
  }
  expect(JSON.stringify(compositionState.document)).toBe(before)
  compositionState.ui.selectedKeyframes = [{ target: { kind: 'visual', clipId: id }, property: 'scaleX', ticks: [100_000, 500_000] }]
  setCompositionKeyframeSelectionInterpolation('ease_in')
  const eased = clip()
  if (eased.kind === 'audio') throw new Error('visual required')
  expect(eased.animation?.scaleX).toEqual(eased.animation?.scaleY)
  expect(eased.scaleLinked).toBe(true)
  undoComposition()
  compositionState.ui.selectedKeyframes = [{ target: { kind: 'visual', clipId: id }, property: 'scaleX', ticks: [100_000, 500_000] }]
  deleteCompositionKeyframeSelection(compositionState.ui.selectedKeyframes)
  const deleted = clip()
  if (deleted.kind === 'audio') throw new Error('visual required')
  expect(deleted.animation?.scaleX).toEqual(deleted.animation?.scaleY)
  expect(deleted.animation?.scaleX).toEqual({ mode: 'constant', value: 2 })
  expect(deleted.scaleLinked).toBe(true)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it.each(['scaleX', 'scaleY', 'both', 'divergent'] as const)('pastes copied %s into linked Scale atomically', (selection) => {
  const property = selection === 'both' || selection === 'divergent' ? 'scaleX' : selection
  applyCompositionVisualAnimation('a-clip', { [property]: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'ease_in', keyframes: [{ tick: 100_000, value: 2 }, { tick: 500_000, value: 3, interpolation: 'hold' }] } } })
  compositionState.ui.selectedKeyframes = [{ target: { kind: 'visual', clipId: 'a-clip' }, property, ticks: [100_000, 500_000] }]
  expect(copyCompositionKeyframeSelection()).toBe(true)
  if (selection === 'both' || selection === 'divergent') {
    const slot = compositionState.ui.clipboard
    if (slot?.kind !== 'keyframes') throw new Error('keyframe copy required')
    compositionState.ui.clipboard = { kind: 'keyframes', groups: [...slot.groups, { ...slot.groups[0]!, parameter: '["visual","scaleY"]', keys: slot.groups[0]!.keys.map((key) => ({ ...key, value: key.value + (selection === 'divergent' ? 7 : 0) })) }] }
  }
  setCompositionScaleLinked('b-clip', true)
  selectCompositionClip('b', 'b-clip', { single: true })
  compositionState.transport.playheadTicks = 1_250_000
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(pasteCompositionKeyframeClipboard()).toBe(true)
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === 'b-clip')!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.scaleLinked).toBe(true)
  expect(clip.animation?.scaleX).toEqual(clip.animation?.scaleY)
  expect(clip.animation?.scaleX).toMatchObject({ track: { keyframes: [{ tick: 250_000, value: selection === 'divergent' ? 9 : 2, interpolation: 'ease_in' }, { tick: 650_000, value: selection === 'divergent' ? 10 : 3, interpolation: 'hold' }] } })
  expect(compositionState.ui.selectedKeyframes).toEqual([{ target: { kind: 'visual', clipId: 'b-clip' }, property: 'scaleX', ticks: [250_000, 650_000] }])
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('drops replaced Y key selection and owned scale previews when linking', () => {
  applyCompositionVisualAnimation('a-clip', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 100_000, value: 2 }] } }, scaleY: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 100_000, value: 3 }] } } })
  const visual = { kind: 'visual' as const, clipId: 'a-clip' }
  compositionState.ui.selectedKeyframes = [{ target: visual, property: 'scaleX', ticks: [100_000] }, { target: visual, property: 'scaleY', ticks: [100_000] }]
  compositionState.ui.keyframePreviews = [{ target: visual, property: 'scaleY', ticks: [], value: { mode: 'constant', value: 9 } }, { target: { kind: 'visual', clipId: 'b-clip' }, property: 'y', ticks: [], value: { mode: 'constant', value: 5 } }]
  setCompositionScaleLinked('a-clip', true)
  expect(compositionState.ui.selectedKeyframes).toEqual([{ target: visual, property: 'scaleX', ticks: [100_000] }])
  expect(compositionState.ui.keyframePreviews).toHaveLength(1)
  expect(compositionState.ui.keyframePreviews[0]!.target.clipId).toBe('b-clip')
})

it('freezes linked Scale at the sampled playhead value rather than the static fallback', () => {
  const staticBefore = JSON.stringify(compositionState.document)
  const staticDepth = compositionState.history.past.length
  clearCompositionAutomation({ kind: 'visual', clipId: 'a-clip' }, 'opacity')
  expect(JSON.stringify(compositionState.document)).toBe(staticBefore)
  expect(compositionState.history.past.length).toBe(staticDepth)
  applyCompositionVisualAnimation('a-clip', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'ease_in', keyframes: [{ tick: 0, value: 1 }, { tick: 1_000_000, value: 3 }] } } })
  setCompositionScaleLinked('a-clip', true)
  compositionState.transport.playheadTicks = 500_000
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  clearCompositionAutomation({ kind: 'visual', clipId: 'a-clip' }, 'scaleX', true)
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === 'a-clip')!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toEqual({ mode: 'constant', value: 1.25 })
  expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
  expect(clip.scaleLinked).toBe(true)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})


it('inserts a linked Scale key with inherited per-key easing in one commit', () => {
  applyCompositionVisualAnimation('a-clip', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 250_000, value: 1, interpolation: 'ease_in' }, { tick: 1_000_000, value: 3, interpolation: 'hold' }] } } })
  setCompositionScaleLinked('a-clip', true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'scaleX', 2, 500_000, true)
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === 'a-clip')!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toMatchObject({ track: { keyframes: [{ tick: 250_000, interpolation: 'ease_in' }, { tick: 500_000, value: 2, interpolation: 'ease_in' }, { tick: 1_000_000, interpolation: 'hold' }] } })
  expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
  expect(clip.scaleLinked).toBe(true)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})


it('preserves pasted keys before a destination clip start as signed local ticks', () => {
  copyCompositionKeyframeSelection()
  compositionState.ui.selectedClipIds = ['a-clip', 'b-clip']
  compositionState.transport.playheadTicks = 0
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(pasteCompositionKeyframeClipboard()).toBe(true)
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === 'b-clip')!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.x).toMatchObject({ track: { keyframes: [{ tick: -1_000_000, value: 0 }] } })
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})


it('pastes keys beyond a clip end without extending its duration', () => {
  copyCompositionKeyframeSelection()
  compositionState.ui.selectedClipIds = ['a-clip']
  compositionState.transport.playheadTicks = 2_000_000
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(pasteCompositionKeyframeClipboard()).toBe(true)
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === 'a-clip')!
  if (clip.kind !== 'text') throw new Error('text fixture required')
  expect(clip.animation?.y).toMatchObject({ track: { keyframes: [{ tick: 3_400_000 }] } })
  expect(clip.durationTicks).toBe(2_000_000)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('renders repeating Loop tails with a vertical cycle reset in the expanded lane', async () => {
  applyCompositionVisualAnimation('a-clip', { x: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: 'loop', after: 'loop' }, keyframes: [{ tick: 500, value: 10 }, { tick: 1000, value: 20 }] } } })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const row = target.querySelector<HTMLElement>('[data-keyframe-track="a"]')!
  row.querySelector<HTMLButtonElement>('.composition-track-head button')!.click()
  await tick()
  const tails = [...row.querySelectorAll<SVGPolylineElement>('.composition-keyframe-tail')]
  expect(tails).toHaveLength(2)
  const points = tails[1]!.getAttribute('points')!.split(' ').map(point => point.split(',').map(Number))
  expect(points.length).toBeGreaterThan(64)
  expect(points.some((point, index) => index > 0 && point[0] === points[index - 1]![0] && Math.abs(point[1]! - points[index - 1]![1]!) > 90)).toBe(true)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  await unmount(component)
})

it('selects only surviving keys when dual-axis paste replaces linked Scale at different times', () => {
  applyCompositionVisualAnimation('a-clip', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 100_000, value: 2 }, { tick: 500_000, value: 3 }] } } })
  compositionState.ui.selectedKeyframes = [{ target: { kind: 'visual', clipId: 'a-clip' }, property: 'scaleX', ticks: [100_000, 500_000] }]
  expect(copyCompositionKeyframeSelection()).toBe(true)
  const slot = compositionState.ui.clipboard
  if (slot?.kind !== 'keyframes') throw new Error('keys required')
  compositionState.ui.clipboard = { kind: 'keyframes', groups: [...slot.groups, { ...slot.groups[0]!, parameter: '["visual","scaleY"]', keys: slot.groups[0]!.keys.map(key => ({ ...key, tick: key.tick + 200_000, value: key.value + 7 })) }] }
  setCompositionScaleLinked('b-clip', true)
  selectCompositionClip('b', 'b-clip', { single: true })
  compositionState.transport.playheadTicks = 1_250_000
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(pasteCompositionKeyframeClipboard()).toBe(true)
  const landed = JSON.stringify(compositionState.document)
  expect(compositionState.ui.selectedKeyframes).toEqual([{ target: { kind: 'visual', clipId: 'b-clip' }, property: 'scaleX', ticks: [450_000, 850_000] }])
  // The resulting selection must immediately remain usable for another copy.
  expect(copyCompositionKeyframeSelection()).toBe(true)
  expect(compositionState.ui.clipboard).toMatchObject({ kind: 'keyframes', groups: [{ parameter: '["visual","scaleX"]', keys: [{ tick: 0, value: 9 }, { tick: 400_000, value: 10 }] }] })
  expect(JSON.stringify(compositionState.document)).toBe(landed)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it.each([false,true])('drags a tangent with draft isolation and exact Undo/Escape: %s', async cancel => {
  applyCompositionVisualAnimation('a-clip', { x: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: { kind: 'bezier', p1: [0.25,0.1], p2: [0.75,0.9] }, keyframes: [{ tick: 0, value: 0 }, { tick: 1_000_000, value: 10 }] } } })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const expand = target.querySelector<HTMLButtonElement>('.composition-keyframe-row .composition-track-head button')!
  expand.click()
  await tick()
  const handle = target.querySelector<SVGCircleElement>('[aria-label="Касательная x outgoing 0"]')!
  expect(handle).not.toBeNull()
  const svg = handle.ownerSVGElement!
  svg.getBoundingClientRect = () => new DOMRect(0,0,Number(svg.getAttribute('width')),120)
  const x = Number(handle.getAttribute('cx')), y = Number(handle.getAttribute('cy'))
  handle.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true, button: 0, clientX: x, clientY: y }))
  window.dispatchEvent(new MouseEvent('pointermove',{ clientX: x+10, clientY: y-20 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(1)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  expect(Number(handle.getAttribute('cx'))).toBeCloseTo(x+10,8)
  expect(Number(handle.getAttribute('cy'))).toBeCloseTo(y-20,8)
  if (cancel) window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape' }))
  window.dispatchEvent(new MouseEvent('pointerup',{ clientX: x+10, clientY: y-20 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  if (!cancel) { expect(compositionState.history.past.length).toBe(depth+1); undoComposition(); await tick() }
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  await unmount(component)
})

it('edits tangent coordinates with keyboard steps and exact Undo', async () => {
  applyCompositionVisualAnimation('a-clip', { x: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: { kind: 'bezier', p1: [0.25,0.1], p2: [0.75,0.9] }, keyframes: [{ tick: 0, value: 0 }, { tick: 1_000_000, value: 10 }] } } })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLButtonElement>('.composition-keyframe-row .composition-track-head button')!.click()
  await tick()
  const handle = target.querySelector<SVGCircleElement>('[aria-label="Касательная x outgoing 0"]')!
  expect(handle.getAttribute('tabindex')).toBe('0')
  handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight', bubbles: true }))
  await tick()
  const clip = compositionState.document.tracks[0]!.clips[0]!
  if (clip.kind !== 'text') throw new Error('text required')
  expect(clip.animation!.x).toMatchObject({ track: { keyframes: [{ tangents: { outgoing: { x: 0.26, y: 0.1, mode: 'free' } } }, {}] } })
  expect(compositionState.history.past.length).toBe(depth+1)
  handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowUp', shiftKey: true, bubbles: true }))
  await tick()
  expect(compositionState.history.past.length).toBe(depth+2)
  undoComposition(); undoComposition()
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'F10', shiftKey: true, bubbles: true }))
  await tick()
  const menu = target.querySelector<HTMLElement>('[aria-label="Непрерывность касательной"]')!
  expect(menu).not.toBeNull()
  const smooth = menu.querySelector<HTMLButtonElement>('button')!
  const broken = menu.querySelectorAll<HTMLButtonElement>('button')[1]!
  expect(broken.getAttribute('aria-checked')).toBe('true')
  smooth.dispatchEvent(new KeyboardEvent('keydown',{ key: 'End', bubbles: true }))
  expect(document.activeElement).toBe(broken)
  broken.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowDown', bubbles: true }))
  expect(document.activeElement).toBe(smooth)
  smooth.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Enter', bubbles: true }))
  await tick()
  expect(compositionState.history.past.length).toBe(depth+1)
  undoComposition()
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  for (const key of ['Escape','Tab']) {
    handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'F10', shiftKey: true, bubbles: true }))
    await tick()
    target.querySelector<HTMLButtonElement>('.composition-tangent-menu button')!.dispatchEvent(new KeyboardEvent('keydown',{ key, bubbles: true }))
    await tick()
    expect(target.querySelector('.composition-tangent-menu')).toBeNull()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past.length).toBe(depth)
  }
  compositionState.document = setClipLocked(compositionState.document, 'a-clip', true)
  await tick()
  const lockedBefore = JSON.stringify(compositionState.document)
  expect(handle.getAttribute('tabindex')).toBe('-1')
  handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight', bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(lockedBefore)
  expect(compositionState.history.past.length).toBe(depth)
  await unmount(component)
})

it.each(['release','escape','lock','concurrent'] as const)('isolates linked tangent drafts and handles %s', async action => {
  applyCompositionVisualAnimation('a-clip', { scaleX: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: { kind: 'bezier', p1: [0.25,0.1], p2: [0.75,0.9] }, keyframes: [{ tick: 0, value: 1 }, { tick: 1_000_000, value: 2 }] } } })
  setCompositionScaleLinked('a-clip',true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  await tick()
  target.querySelector<HTMLButtonElement>('.composition-keyframe-row .composition-track-head button')!.click()
  await tick()
  const handle = target.querySelector<SVGCircleElement>('[aria-label="Касательная scaleX outgoing 0"]')!
  const svg = handle.ownerSVGElement!
  svg.getBoundingClientRect = () => new DOMRect(0,0,Number(svg.getAttribute('width')),120)
  const x = Number(handle.getAttribute('cx')), y = Number(handle.getAttribute('cy'))
  handle.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true, button: 0, clientX: x, clientY: y }))
  window.dispatchEvent(new MouseEvent('pointermove',{ clientX: x+10, clientY: y-20 }))
  await tick()
  const previews = compositionState.ui.keyframePreviews
  expect(previews.map(p => p.property).sort()).toEqual(['scaleX','scaleY'])
  expect(previews[0]!.value).toEqual(previews[1]!.value)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  if (action === 'escape') window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape' }))
  if (action === 'lock') compositionState.document = setClipLocked(compositionState.document,'a-clip',true)
  if (action === 'concurrent') compositionState.document = { ...compositionState.document, canvas: { ...compositionState.document.canvas, backgroundColor: '#111111' } }
  await tick()
  const guarded = JSON.stringify(compositionState.document)
  if (action === 'lock' || action === 'concurrent') expect(compositionState.ui.keyframePreviews).toEqual([])
  window.dispatchEvent(new MouseEvent('pointerup',{ clientX: x+10, clientY: y-20 }))
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  if (action === 'release') {
    const clip = compositionState.document.tracks[0]!.clips[0]!
    if (clip.kind !== 'text') throw new Error('text required')
    expect(clip.animation!.scaleX).toEqual(previews[0]!.value)
    expect(clip.animation!.scaleY).toEqual(previews[0]!.value)
    expect(clip.scaleLinked).toBe(true)
    expect(compositionState.history.past.length).toBe(depth+1)
    undoComposition(); await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } else expect(JSON.stringify(compositionState.document)).toBe(guarded)
  expect(compositionState.history.past.length).toBe(depth)
  await unmount(component)
})

it('exposes all named presets and commits a preview across selected lanes with exact Undo', async () => {
  const { BEZIER_PRESETS } = await import('$lib/composition/easingPresets')
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  target.querySelector<HTMLElement>('.composition-keyframe-diamond')!.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true,cancelable: true }))
  await tick()
  const options = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')]
  for (const preset of BEZIER_PRESETS) expect(options.some(option => option.textContent === preset.id)).toBe(true)
  const quad = options.find(option => option.textContent === 'ease_in_quad')!
  quad.focus()
  await tick()
  expect(compositionState.ui.keyframePreviews).toHaveLength(2)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
  quad.click()
  await tick()
  expect(compositionState.ui.keyframePreviews).toEqual([])
  expect(compositionState.history.past).toHaveLength(depth+1)
  for (const track of compositionState.document.tracks) {
    const clip = track.clips[0]!
    if (clip.kind !== 'text') throw new Error('text required')
    const value = clip.animation![track.id === 'a' ? 'x' : 'y']!
    if (value.mode !== 'keyframes') throw new Error('keys required')
    expect(value.track.keyframes[0]!.interpolation).toEqual(BEZIER_PRESETS.find(preset => preset.id === 'ease_in_quad')!.interpolation)
  }
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('shows native color keys on the timeline and moves/deletes them with exact Undo', async () => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => track.id !== 'a' ? track : { ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' }] } } })) }) } as typeof compositionState.document
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    let key = target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!
    expect(key).toBeTruthy()
    expect(Number.parseFloat(key.style.left)).toBeCloseTo(compositionState.ui.zoomPxPerSecond*0.1)
    key.click();await tick()
    expect(compositionState.transport.playheadTicks).toBe(100000)
    key.focus()
    key.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }));await tick()
    key = target.querySelector<HTMLButtonElement>('[data-color-key-tick="133"]')!
    expect(key).toBeTruthy()
    expect(key.title).toContain('#FF000080')
    expect(document.activeElement).toBe(key)
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    key = target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!
    key.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Delete',bubbles: true }));await tick()
    expect(target.querySelector('[data-color-key-tick="100"]')).toBeNull()
    expect(target.querySelector('[data-color-key-tick="200"]')).toBeTruthy()
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component) }
})

it.each(['commit','cancel','collision','concurrent'] as const)('previews color key drag and guards its release: %s', async mode => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => track.id !== 'a' ? track : { ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' }] } } })) }) } as typeof compositionState.document
  const { colorCurvePreview } = await import('$lib/state/colorCurvePreview.svelte')
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    const key = target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!
    key.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true,button: 0,clientX: 0 }))
    const x = compositionState.ui.zoomPxPerSecond*(mode === 'collision' ? 0.1 : 0.05)
    window.dispatchEvent(new MouseEvent('pointermove',{ clientX: x }));await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    if (mode !== 'collision') expect(colorCurvePreview.document).not.toBeNull()
    if (mode === 'concurrent') compositionState.document = { ...compositionState.document,canvas: { ...compositionState.document.canvas,backgroundColor: '#123456' } }
    const expected = JSON.stringify(compositionState.document)
    if (mode === 'cancel') window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape' }))
    else window.dispatchEvent(new MouseEvent('pointerup',{ clientX: x }))
    await tick()
    expect(colorCurvePreview.document).toBeNull()
    if (mode === 'commit') {
      expect(target.querySelector('[data-color-key-tick="150"]')).toBeTruthy()
      expect(compositionState.history.past).toHaveLength(depth+1)
      undoComposition();await tick()
      expect(JSON.stringify(compositionState.document)).toBe(before)
    } else {
      expect(JSON.stringify(compositionState.document)).toBe(expected)
      expect(compositionState.history.past).toHaveLength(depth)
    }
  } finally { await unmount(component) }
})

it('Shift-selects color keys and moves or deletes the group in one Undo', async () => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => track.id !== 'a' ? track : { ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' }] } } })) }) } as typeof compositionState.document
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    const selectPair = async () => {
      target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!.click()
      target.querySelector<HTMLButtonElement>('[data-color-key-tick="200"]')!.dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }))
      await tick()
      expect(target.querySelectorAll('.color-key[aria-pressed="true"]')).toHaveLength(2)
    }
    await selectPair()
    target.querySelector('[data-color-key-tick="100"]')!.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }));await tick()
    expect(target.querySelector('[data-color-key-tick="133"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="233"]')).toBeTruthy()
    expect(target.querySelectorAll('.color-key[aria-pressed="true"]')).toHaveLength(2)
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await selectPair()
    const anchor = target.querySelector('[data-color-key-tick="100"]')!
    anchor.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true,button: 0,clientX: 0 }))
    window.dispatchEvent(new MouseEvent('pointermove',{ clientX: compositionState.ui.zoomPxPerSecond*0.05 }));await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    window.dispatchEvent(new MouseEvent('pointerup',{ clientX: compositionState.ui.zoomPxPerSecond*0.05 }));await tick()
    expect(target.querySelector('[data-color-key-tick="150"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="250"]')).toBeTruthy()
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await selectPair()
    target.querySelector('[data-color-key-tick="100"]')!.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Delete',bubbles: true }));await tick()
    expect(target.querySelectorAll('.color-key')).toHaveLength(0)
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component) }
})

it('moves selected color keys across timeline tracks with different authored clocks', async () => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => ({ ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: track.id === 'a' ? 1000 : 48000,interpolation: 'linear',keyframes: [{ tick: track.id === 'a' ? 100 : 4800,value: '#FF000080' },{ tick: track.id === 'a' ? 200 : 9600,value: '#0000FFFF' }] } } })) })) } as typeof compositionState.document
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!.click()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="4800"]')!.dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }));await tick()
    expect(compositionState.ui.selectedColorKeys).toHaveLength(2)
    target.querySelector('[data-color-key-tick="100"]')!.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }));await tick()
    expect(target.querySelector('[data-color-key-tick="133"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="6400"]')).toBeTruthy()
    expect(compositionState.history.past).toHaveLength(depth+1)
    const anchor = target.querySelector('[data-color-key-tick="133"]')!
    anchor.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true,button: 0,clientX: 0 }))
    window.dispatchEvent(new MouseEvent('pointermove',{ clientX: compositionState.ui.zoomPxPerSecond*0.1 }));await tick()
    const other = target.querySelector<HTMLElement>('[data-color-key-tick="6400"]')!
    expect(Number.parseFloat(other.style.left)).toBeCloseTo(compositionState.ui.zoomPxPerSecond*(1+11200/48000))
    window.dispatchEvent(new MouseEvent('pointerup',{ clientX: compositionState.ui.zoomPxPerSecond*0.1 }));await tick()
    expect(target.querySelector('[data-color-key-tick="233"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="11200"]')).toBeTruthy()
    expect(compositionState.history.past).toHaveLength(depth+2)
    undoComposition();undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!.click()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="4800"]')!.dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }));await tick()
    target.querySelector('[data-color-key-tick="4800"]')!.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true,button: 0,clientX: 0,altKey: true }))
    window.dispatchEvent(new MouseEvent('pointermove',{ clientX: compositionState.ui.zoomPxPerSecond }));await tick()
    window.dispatchEvent(new MouseEvent('pointerup',{ clientX: compositionState.ui.zoomPxPerSecond }));await tick()
    expect(target.querySelector('[data-color-key-tick="100"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="52800"]')).toBeTruthy()
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component) }
})

it.each(['commit','cancel','invalid'] as const)('Alt-scales selected color key times with transactional release: %s', async mode => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => track.id !== 'a' ? track : { ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' }] } } })) }) } as typeof compositionState.document
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!.click()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="200"]')!.dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }));await tick()
    target.querySelector('[data-color-key-tick="200"]')!.dispatchEvent(new MouseEvent('pointerdown',{ bubbles: true,button: 0,clientX: 0,altKey: true }))
    const x = compositionState.ui.zoomPxPerSecond*(mode === 'invalid' ? -0.2 : 0.1)
    window.dispatchEvent(new MouseEvent('pointermove',{ clientX: x }));await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    if (mode === 'cancel') window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape' }))
    else window.dispatchEvent(new MouseEvent('pointerup',{ clientX: x }))
    await tick()
    if (mode === 'commit') {
      expect(target.querySelector('[data-color-key-tick="100"]')).toBeTruthy()
      expect(target.querySelector('[data-color-key-tick="300"]')).toBeTruthy()
      expect(compositionState.history.past).toHaveLength(depth+1)
      undoComposition();await tick()
    } else expect(compositionState.history.past).toHaveLength(depth)
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component) }
})

it('copies and pastes selected color keys through the shared editor shortcut slot', async () => {
  compositionState.document = { ...compositionState.document,tracks: compositionState.document.tracks.map(track => track.id !== 'a' ? track : { ...track,clips: track.clips.map(clip => ({ ...clip,colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 100,value: '#FF000080' },{ tick: 200,value: '#0000FFFF' }] } } })) }) } as typeof compositionState.document
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const component = mount(MultiTrackTimeline,{ target })
  try {
    await tick()
    const first = target.querySelector<HTMLButtonElement>('[data-color-key-tick="100"]')!
    first.click()
    target.querySelector<HTMLButtonElement>('[data-color-key-tick="200"]')!.dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }));await tick()
    first.dispatchEvent(new KeyboardEvent('keydown',{ key: 'c',code: 'KeyC',ctrlKey: true,bubbles: true }));await tick()
    expect(compositionState.ui.clipboard?.kind).toBe('color-keyframes')
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    compositionState.transport.playheadTicks = 500000
    first.dispatchEvent(new KeyboardEvent('keydown',{ key: 'v',code: 'KeyV',ctrlKey: true,bubbles: true }));await tick()
    expect(target.querySelector('[data-color-key-tick="500"]')).toBeTruthy()
    expect(target.querySelector('[data-color-key-tick="600"]')).toBeTruthy()
    expect(compositionState.ui.selectedColorKeys.map(subject => subject.tick)).toEqual([500,600])
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component) }
})

it('inserts into a clocked curve at the cursor and projects the authored key through the clock with Undo', async () => {
  applyCompositionVisualAnimation('a-clip', { x: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, value: 0 }, { tick: 1250, value: 10 }] } } })
  const before = JSON.stringify(compositionState.document)
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: 'a-clip' }, 'x', 7, 100000)
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const diamond = target.querySelector<HTMLElement>('[data-keyframe-track="a"] [data-keyframe-tick="450"]')
  expect(diamond).not.toBeNull()
  expect(Number.parseFloat(diamond!.style.left)).toBeCloseTo(0.1 * compositionState.ui.zoomPxPerSecond, 6)
  undoComposition()
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(target.querySelector('[data-keyframe-track="a"] [data-keyframe-tick="450"]')).toBeNull()
  await unmount(component)
})
