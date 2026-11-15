import {agentSession} from '$lib/agent/session.svelte'
import { serializeNativeClipboard,parseNativeClipboard } from '$lib/composition/nativeClipboardText'
import { shortcutState } from '$lib/state/shortcuts.svelte'
import fixture from '../../../../../backend/tests/fixtures/concat-merge-preview.json'
// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import { state as library } from '$lib/state/store.svelte'
import NestedClipTransfer from './NestedClipTransfer.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, trimNestedClipEnds, nestedClipboard } from '$lib/state/nestedComposition.svelte'
import { addClip, addTrack, createComposition, registerSource } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { cuesToTextClips, parseSrt } from '$lib/subtitles/srt'
vi.mock('$lib/api', () => ({ uploadFile: vi.fn(), editCompositionCaptions: vi.fn(), moveNativeNestedClips: vi.fn(), nestedDurations: vi.fn(async () => ({})), evaluateNestedFrame: vi.fn(async () => ({})) }))

it('edits the selected file caption and keeps its submicrosecond clock through Undo', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
  const cue = { id: 'cue', cueId: 'external', timelineStartTick: '254016000001', timelineEndTick: '508032000003',
    text: '<i>Original</i>', speaker: 'Original voice', settings: 'line:80%' }
  const nextCue = { id: 'next', timelineStartTick: 3_000_000, timelineEndTick: 4_000_000, text: 'Next' }
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: {
    instances: [], content: { ...content, captionTracks: [{ id: 'file', name: 'Captions', clips: [cue, nextCue] }] },
  } } }
  const original = JSON.stringify(nestedComposition.document)
  vi.mocked(api.nestedDurations).mockClear()
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const select = target.querySelector<HTMLInputElement>('[data-caption-id="cue"]')!
    select.checked = true
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const editor = target.querySelector('[aria-label="Редактировать реплику дорожки"]')!
    const fields = editor.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('textarea,input')
    for (const [index, value] of ['<b>Edited</b>', 'New voice', 'changed-id', 'line:90% align:start'].entries()) {
      fields[index]!.value = value
      fields[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    await tick()
    editor.querySelector('button')!.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    expect(api.nestedDurations).toHaveBeenCalledTimes(1)
    expect(nestedComposition.document?.compositions.root?.content.captionTracks?.[0]?.clips[0]).toEqual({
      ...cue, text: '<b>Edited</b>', speaker: 'New voice', cueId: 'changed-id', settings: 'line:90% align:start',
    })
    await vi.waitFor(() => expect(editor.querySelector<HTMLButtonElement>('button')!.disabled).toBe(true))
    const setTime = async (index: number, value: string) => {
      const field = fields[index]!
      field.value = value
      field.dispatchEvent(new Event('input', { bubbles: true }))
      await tick()
      editor.querySelector('button')!.click()
    }
    await setTime(5, '3,1')
    await vi.waitFor(() => expect(editor.textContent).toContain('не могут перекрываться'))
    expect(nestedHistory.past).toHaveLength(1)
    await setTime(5, '2,500000000004')
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
    expect(nestedComposition.document?.compositions.root?.content.captionTracks?.[0]?.clips[0]).toMatchObject({
      timelineStartTick: cue.timelineStartTick, timelineEndTick: '635040000001',
    })
    await vi.waitFor(() => expect(editor.querySelector<HTMLButtonElement>('button')!.disabled).toBe(true))
    await setTime(4, '1.250000000004')
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
    expect(nestedComposition.document?.compositions.root?.content.captionTracks?.[0]?.clips[0]).toMatchObject({
      timelineStartTick: '317520000001', timelineEndTick: '635040000001',
    })
    undoNestedComposition()
    undoNestedComposition()
    undoNestedComposition()
    expect(JSON.stringify(nestedComposition.document)).toBe(original)
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
  }
})

it('adds a cue at the cursor on the chosen imported track and opens its editor', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
  const track = { id: 'file', name: 'Импорт', clips: [] }
  const locked = { id: 'locked', name: 'Защищённая', clips: [], locked: true }
  const original = { ...content, captionTracks: [track, locked] }
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: {
    root: { instances: [], content: original, durationUs: 2_000_000 },
  } }
  nestedComposition.timeUs = 1_508_000
  const cue = { id: 'new-cue', timelineStartTick: '381024000000', timelineEndTick: '1143072000000', text: 'Субтитр' }
  const mock = vi.mocked(api.editCompositionCaptions)
  mock.mockReset().mockResolvedValueOnce({
    composition: { ...original, captionTracks: [{ ...track, clips: [cue] }, locked] },
    appliedDelta: null, deleted: 0,
  })
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const button = (name: string) => target.querySelector<HTMLButtonElement>(`[aria-label="Добавить реплику на курсоре · ${name}"]`)!
    expect(button('Защищённая').disabled).toBe(true)
    button('Импорт').click()
    await vi.waitFor(() => expect(target.querySelector('[aria-label="Редактировать реплику дорожки"]')).not.toBeNull())
    expect(mock).toHaveBeenCalledWith(original, { kind: 'add', trackId: 'file', startTick: 1_508_000, text: 'Субтитр' })
    expect(nestedComposition.document?.compositions.root?.durationUs).toBe(4_500_000)
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition()
    expect(nestedComposition.document?.compositions.root).toEqual({ instances: [], content: original, durationUs: 2_000_000 })
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
    mock.mockReset()
  }
})

it('edits imported captions through native commands and shared Undo', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
  const first = { id: 'cue', cueId: 'external', timelineStartTick: '254016000001', timelineEndTick: '508032000003', text: 'First' }
  const next = { id: 'next', timelineStartTick: '762048000005', timelineEndTick: '1016064000007', text: 'Next' }
  const track = { id: 'file', name: 'Imported captions', clips: [first, next] }
  const original = { ...content, captionTracks: [track] }
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { instances: [], content: original, durationUs: 4_100_000 } } }
  nestedComposition.timeUs = 1_500_000
  const left = { ...first, timelineEndTick: 1_500_000 }
  const right = { ...first, id: 'right', cueId: null, timelineStartTick: 1_500_000 }
  const joined = { ...first, text: 'First\nFirst' }
  const shifted = { ...next, timelineStartTick: '508032000003', timelineEndTick: '762048000005' }
  const moved = { ...shifted, timelineStartTick: '889056000004', timelineEndTick: '1143072000006' }
  const trimmed = { ...moved, timelineEndTick: '1132488000006' }
  const mock = vi.mocked(api.editCompositionCaptions)
  mock.mockReset()
  for (const clips of [[left, right, next], [joined, next], [shifted], [moved], [trimmed]]) {
    mock.mockResolvedValueOnce({ composition: { ...original, captionTracks: [{ ...track, name: 'Server track', clips }] }, appliedDelta: null, deleted: 0 })
  }
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  const select = async (id: string) => {
    const input = target.querySelector<HTMLInputElement>(`[data-caption-id="${id}"]`)!
    input.checked = true
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
  }
  const click = (name: string) => [...target.querySelectorAll('button')].find(button => button.textContent?.trim() === name)!.click()
  const currentTrack = () => nestedComposition.document?.compositions.root?.content.captionTracks?.[0]
  try {
    await tick()
    await select('cue')
    click('Разрезать реплику на курсоре')
    await vi.waitFor(() => expect(currentTrack()).toEqual({ ...track, clips: [left, right, next] }))
    expect(mock.mock.calls[0]?.[1]).toEqual({ kind: 'split', id: 'cue', timeTick: 1_500_000 })
    await select('right')
    click('Объединить реплики дорожки')
    await vi.waitFor(() => expect(currentTrack()?.clips).toEqual([joined, next]))
    expect(mock.mock.calls[1]?.[1]).toEqual({ kind: 'merge', ids: ['cue', 'right'] })
    await tick()
    click('Удалить реплики со сдвигом')
    await vi.waitFor(() => expect(currentTrack()?.clips).toEqual([shifted]))
    expect(mock.mock.calls[2]?.[1]).toEqual({ kind: 'delete', ids: ['cue'], ripple: true })
    await select('next')
    const delta = [...target.querySelectorAll('label')].find(label => label.textContent?.trim() === 'Сдвиг реплик')!.querySelector('input')!
    delta.value = '1,499999999996'
    delta.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    click('Сдвинуть реплики')
    await vi.waitFor(() => expect(currentTrack()?.clips).toEqual([moved]))
    expect(mock.mock.calls[3]?.[1]).toEqual({ kind: 'move', ids: ['next'], deltaTicks: '381023999999' })
    expect(nestedComposition.document?.compositions.root?.durationUs).toBe(4_500_001)
    const unit = [...target.querySelectorAll('select')].find(select => select.options[0]?.value === 'seconds')!
    unit.value = 'frames'
    unit.dispatchEvent(new Event('change', { bubbles: true }))
    delta.value = '-1'
    delta.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    click('Обрезать конец реплики')
    await vi.waitFor(() => expect(currentTrack()?.clips).toEqual([trimmed]))
    expect(mock.mock.calls[4]?.[1]).toEqual({ kind: 'trim', id: 'next', inEdge: false, deltaTicks: '-10584000000' })
    expect(nestedHistory.past).toHaveLength(5)
    undoNestedComposition()
    expect(currentTrack()?.clips).toEqual([moved])
    undoNestedComposition()
    expect(currentTrack()?.clips).toEqual([shifted])
    expect(nestedComposition.document?.compositions.root?.durationUs).toBe(4_100_000)
    undoNestedComposition()
    expect(currentTrack()?.clips).toEqual([joined, next])
    undoNestedComposition()
    undoNestedComposition()
    expect(currentTrack()).toEqual(track)
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
    mock.mockReset()
  }
})

it('applies a chosen subtitle appearance to multiple selected replies', async () => {
  closeNestedComposition()
  const cues = cuesToTextClips([
    { id: 'cue-0', startTicks: 0, endTicks: 500000, text: 'First' },
    { id: 'cue-1', startTicks: 500000, endTicks: 1000000, text: 'Reference' },
    { id: 'cue-2', startTicks: 1000000, endTicks: 1500000, text: 'Third' },
  ], cue => cue.id).map((cue, index) => ({ ...cue, style: { ...cue.style, fontSizePx: 24 + index * 8 } }))
  const base = createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' })
  const draft = addTrack(base, { id: 'captions', kind: 'text', name: 'Subtitles', hidden: false, locked: false, clips: cues })
  const content = buildCompositionWireDocument({ ...draft, metadata: { captionTextTracks: ['captions'] } })
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.activeCompositionId = 'root'
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    for (const checkbox of target.querySelectorAll<HTMLInputElement>('[data-clip-id]')) {
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    await tick()
    const picker = [...target.querySelectorAll('select')].find(select => select.options[0]?.textContent === 'Выберите реплику')!
    picker.value = 'cue-1'
    picker.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const apply = [...target.querySelectorAll('button')].find(button => button.textContent?.trim() === 'Применить оформление к другим выбранным')!
    expect(apply.disabled).toBe(false)
    apply.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    const track = nestedComposition.document?.compositions.root?.content.tracks[0]
    expect(track?.clips).toMatchObject([
      { text: 'First', timelineStartTick: 0, style: { fontSize: 32 } },
      { text: 'Reference', timelineStartTick: 500000, style: { fontSize: 32 } },
      { text: 'Third', timelineStartTick: 1000000, style: { fontSize: 32 } },
    ])
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
  }
})

it('saves subtitle text, typography and background together and restores them with one Undo', async () => {
  closeNestedComposition()
  const [cue] = cuesToTextClips(parseSrt('1\n00:00:01,000 --> 00:00:02,000\nOriginal'), () => 'cue')
  const base = createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' })
  const opacityKeys = {
    mode: 'keyframes', track: { timeBase: 1000000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0 }, { tick: 500000, value: 1 }] },
  } as const
  const draft = addTrack(base, { id: 'captions', kind: 'text', name: 'Subtitles', hidden: false, locked: false,
    clips: [{ ...cue!, animation: { ...cue!.animation, opacity: opacityKeys } }],
  })
  const content = buildCompositionWireDocument({ ...draft, metadata: { captionTextTracks: ['captions'] } })
  nestedComposition.document = { schemaVersion: 1, root: 'root', fonts: [{ family: 'Embedded Face', data: 'Zm9udA==' }],
    compositions: { root: { content, instances: [] } },
  }
  nestedComposition.activeCompositionId = 'root'
  const before = JSON.stringify(nestedComposition.document)
  vi.mocked(api.nestedDurations).mockClear()
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const editor = target.querySelector<HTMLDetailsElement>('[data-caption-editor="cue"]')!
    editor.open = true
    const family = editor.querySelector<HTMLInputElement>('input[list]')!
    expect(editor.querySelector('datalist option[value="Embedded Face"]')).not.toBeNull()
    family.value = 'Embedded Face'
    family.dispatchEvent(new Event('input', { bubbles: true }))
    const italic = editor.querySelector<HTMLInputElement>('input[type="checkbox"]')!
    italic.checked = true
    italic.dispatchEvent(new Event('change', { bubbles: true }))
    const alignment = editor.querySelector('select')!
    alignment.value = 'right'
    alignment.dispatchEvent(new Event('change', { bubbles: true }))
    const fields = editor.querySelectorAll<HTMLInputElement>('input[type="number"]')
    for (const [index, value] of ['28', '550.5', '-2', '1.6', '200', '54', '0.6', '4', '8', '6', '0.7', '0.8', '2'].entries()) {
      fields[index]!.value = value
      fields[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    const colors = editor.querySelectorAll<HTMLInputElement>('input[type="color"]')
    for (const [index, value] of ['#0000ff', '#ff0000', '#00ff00'].entries()) {
      colors[index]!.value = value
      colors[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    const shadow = [...editor.querySelectorAll('details')].find(section => section.querySelector('summary')?.textContent === 'Тень текста')!
    shadow.open = true
    const mode = shadow.querySelector('select')!
    mode.value = 'custom'
    mode.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const shadowColor = shadow.querySelector<HTMLInputElement>('input[type="color"]')!
    shadowColor.value = '#0000ff'
    shadowColor.dispatchEvent(new Event('input', { bubbles: true }))
    const shadowFields = shadow.querySelectorAll<HTMLInputElement>('input[type="number"]')
    for (const [index, value] of ['0.4', '-3', '4'].entries()) {
      shadowFields[index]!.value = value
      shadowFields[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    const composite = [...editor.querySelectorAll('details')]
      .find(section => section.querySelector('summary')?.textContent === 'Наложение на кадр')!
    composite.open = true
    const opacity = composite.querySelector('input')!
    opacity.value = '0.45'
    opacity.dispatchEvent(new Event('input', { bubbles: true }))
    const blend = composite.querySelector('select')!
    blend.value = 'screen'
    blend.dispatchEvent(new Event('change', { bubbles: true }))
    const textarea = editor.querySelector('textarea')!
    textarea.value = 'Edited'
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    const framing = [...editor.querySelectorAll('details')]
      .find(section => section.querySelector('summary')?.textContent === 'Кадрирование и отражение')!
    framing.open = true
    const edges = framing.querySelectorAll<HTMLInputElement>('input[type="number"]')
    const setEdge = (index: number, value: string) => {
      edges[index]!.value = value
      edges[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    setEdge(0, '70')
    setEdge(2, '40')
    await tick()
    const save = [...editor.querySelectorAll('button')].find(button => button.textContent === 'Сохранить реплику')!
    expect(save.disabled).toBe(false)
    save.click()
    await vi.waitFor(() => expect(editor.querySelector('[role="alert"]')?.textContent).toContain('10%'))
    expect(api.nestedDurations).not.toHaveBeenCalled()
    expect(nestedHistory.past).toHaveLength(0)
    for (const [index, value] of ['12.5', '10', '20', '5'].entries()) setEdge(index, value)
    for (const flip of framing.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) {
      flip.checked = true
      flip.dispatchEvent(new Event('change', { bubbles: true }))
    }
    const position = [...editor.querySelectorAll('details')]
      .find(section => section.querySelector('summary')?.textContent === 'Положение в кадре')!
    position.open = true
    const positionFields = position.querySelectorAll<HTMLInputElement>('input')
    for (const [index, value] of ['20', '-35', '1.5', '0.75', '30', '25', '75'].entries()) {
      positionFields[index]!.value = value
      positionFields[index]!.dispatchEvent(new Event('input', { bubbles: true }))
    }
    await tick()
    save.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    expect(api.nestedDurations).toHaveBeenCalledTimes(1)
    expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
      text: 'Edited', textBoxWidth: 200, textBoxHeight: 54, opacityMultiplier: 0.45, blendMode: 'screen',
      opacity: opacityKeys, textAnchor: [0.25, 0.75],
      transform: { x: { mode: 'constant', value: 20 }, y: { mode: 'constant', value: -35 },
        scaleX: { mode: 'constant', value: 1.5 }, scaleY: { mode: 'constant', value: 0.75 },
        rotationDegrees: { mode: 'constant', value: 30 },
        concatPreparation: { crop: [0.125, 0.1, 0.2, 0.05], canvas: [320, 180], flipH: true, flipV: true } },
      style: { fontFamily: 'Embedded Face', fontSize: 28, fontWeight: 550.5,
        italic: true, align: 'right', tracking: -2, lineHeight: 1.6,
        color: { red: 1, green: 0, blue: 0, alpha: 0.7 },
        background: { red: 0, green: 0, blue: 1, alpha: 0.6 }, backgroundRadius: 4, backgroundPadding: [8, 6],
        stroke: { red: 0, green: 1, blue: 0, alpha: 0.8 }, strokeWidth: 2 },
    })
    expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
      style: { shadow: { red: 0, green: 0, blue: 1, alpha: 0.4 }, shadowX: -3, shadowY: 4 },
    })
    mode.value = 'concat'
    mode.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    save.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
    expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
      style: { concatShadow: true, shadow: { alpha: 0.4 } },
    })
    mode.value = 'off'
    mode.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    save.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
    expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
      style: { concatShadow: undefined, shadow: { alpha: 0 } },
    })
    for (const field of [positionFields[5]!, positionFields[6]!]) {
      field.value = ''
      field.dispatchEvent(new Event('input', { bubbles: true }))
    }
    await tick()
    save.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(4))
    expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
      textAnchor: undefined, transform: { anchorX: 0.5, anchorY: 0.5 },
    })
    undoNestedComposition()
    undoNestedComposition()
    undoNestedComposition()
    undoNestedComposition()
    expect(JSON.stringify(nestedComposition.document)).toBe(before)
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
  }
})
it('selects a clip, sends a native transfer to another timeline, focuses it and restores everything with Undo', async () => {
  closeNestedComposition()
  let source = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 4_000_000, width: 4, height: 4, hasAudio: false })
  source = addTrack(source, { id: 'v', kind: 'video', name: 'V', hidden: false, muted: false, locked: false, clips: [] })
  source = addClip(source, 'v', { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 2_000_000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
  const empty = addTrack(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'target-v', kind: 'video', name: 'Target V', hidden: false, muted: false, locked: false, clips: [] })
  const content = buildCompositionWireDocument(source), targetContent = buildCompositionWireDocument(empty)
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, target: { content: targetContent, instances: [] } } }
  const changed: api.NativeNestedProject = { ...document, compositions: {
    root: { content: { ...content, tracks: content.tracks.map(track => track.kind === 'video' ? { ...track, clips: [] } : track) }, instances: [] },
    target: { content: { ...targetContent, sources: content.sources, tracks: targetContent.tracks.map(track => track.kind === 'video' ? { ...track, clips: content.tracks.flatMap(item => item.kind === 'video' ? item.clips : []).map(clip => ({ ...clip, placement: { ...clip.placement, timelineStartTick: 2_000_000 } })) } : track) }, instances: [] },
  } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.timeUs = 750000
  vi.mocked(api.moveNativeNestedClips).mockResolvedValue({ project: changed, changed: true, affectedCompositions: ['root', 'target'], movedClipIds: ['picture'] })
  const target = window.document.createElement('div'); window.document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled).toBe(true)
    const checkbox = target.querySelector<HTMLInputElement>('[data-clip-id="picture"]')!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    const start = target.querySelector<HTMLInputElement>('[aria-label="Начало переноса клипов"]')!
    start.value = '2'; start.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    expect(button.disabled).toBe(false)
    button.click()
    await vi.waitFor(() => expect(nestedComposition.activeCompositionId).toBe('target'))
    expect(api.moveNativeNestedClips).toHaveBeenCalledWith(document, [{ fromComposition: 'root', fromTrackId: 'v', clipId: 'picture', toComposition: 'target', toTrackId: 'target-v', startUs: 2_000_000, createTrack: false }])
    expect(nestedComposition.timeUs).toBe(2_000_000)
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition()
    expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.activeCompositionId).toBe('root')
    expect(nestedComposition.timeUs).toBe(750000)
  } finally { await unmount(component); target.remove(); closeNestedComposition() }
})

it('deletes a selected clip with native validation and restores it through Undo', async () => {
  closeNestedComposition()
  let source = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 4000000, width: 4, height: 4, hasAudio: false })
  source = addTrack(source, { id: 'v', kind: 'video', name: 'V', hidden: false, muted: false, locked: false, clips: [] })
  source = addClip(source, 'v', { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 2000000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content: buildCompositionWireDocument(source), instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.renderedOutputs = { root: 'old' }
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const checkbox = target.querySelector<HTMLInputElement>('[data-clip-id="picture"]')!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Удалить выбранные клипы')!
    expect(button.disabled).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips).toHaveLength(0))
    expect(api.evaluateNestedFrame).toHaveBeenCalled()
    expect(nestedHistory.past).toHaveLength(1)
    expect(nestedComposition.renderedOutputs).toEqual({})
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'old' })
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})

it('selects the stored AV pair together and deletes/restores both clips with one Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const video = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    const audio = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first-audio"]')!
    video.checked = true; video.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(audio.checked).toBe(true)
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Удалить выбранные клипы')!
    button.click()
    await vi.waitFor(() => expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks.every(track => track.clips.length === 0)).toBe(true))
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})

it('trims both linked right edges through controls and restores their original clocks with Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const selected = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    selected.checked = true; selected.dispatchEvent(new Event('change', { bubbles: true }))
    const delta = target.querySelector<HTMLInputElement>('[aria-label="Сдвиг правого края клипов"]')!
    delta.value = '-0.5'; delta.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Изменить правый край')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: 4000000, timelineDurationTick: 2500000 } })
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})


it('keeps the linked document, outputs, cursor and history intact when native trim validation refuses', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  nestedComposition.timeUs = 2250000; nestedComposition.renderedOutputs = { root: 'parent-output', 'new-timeline': 'timeline-output' }
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native trim refused'))
  try {
    await trimNestedClipEnds('new-timeline', ['auto-first'], -500000)
    expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.timeUs).toBe(2250000)
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'parent-output', 'new-timeline': 'timeline-output' })
    expect(nestedHistory.past).toHaveLength(0)
    expect(nestedHistory.error).toBe('native trim refused')
  } finally { closeNestedComposition() }
})

it('unlinks and relinks the AV pair through controls without invalidating rendered outputs, with Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  nestedComposition.renderedOutputs = { 'new-timeline': 'rendered' }; nestedComposition.timeUs = 2250000
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const selected = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    selected.checked = true; selected.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = (text: string) => [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === text)!
    expect(button('Связать выбранные клипы').disabled).toBe(true)
    expect(button('Разорвать связь клипов').disabled).toBe(false)
    button('Разорвать связь клипов').click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    expect(nestedComposition.document!.compositions['new-timeline']!.content.metadata!.clipLinks).toEqual({})
    expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'rendered' })
    expect(nestedComposition.timeUs).toBe(2250000)
    await tick()
    expect(button('Связать выбранные клипы').disabled, button('Связать выбранные клипы').title).toBe(false)
    button('Связать выбранные клипы').click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
    expect(nestedComposition.document!.compositions['new-timeline']!.content.metadata!.clipLinks).toEqual(document.compositions['new-timeline']!.content.metadata!.clipLinks)
    undoNestedComposition(); expect(nestedComposition.document!.compositions['new-timeline']!.content.metadata!.clipLinks).toEqual({})
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})
it('splits both linked clips at the cursor and restores them with one Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.timeUs = 3250000
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const selected = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    selected.checked = true; selected.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Разрезать выбранные на курсоре')!
    button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    for (const index of [1, 3]) {
      const clips = nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips
      expect(clips).toHaveLength(2)
      expect(clips[1]).toMatchObject({ placement: { timelineStartTick: 3250000, sourceTimelineOffsetTick: 1250000 } })
    }
    expect(nestedComposition.timeUs).toBe(3250000)
    await tick()
    const selections = [...target.querySelectorAll<HTMLInputElement>('input[type="checkbox"][data-clip-id]')]
    expect(selections).toHaveLength(4)
    expect(selections.every(input => input.checked)).toBe(true)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
    await tick()
    expect(target.querySelectorAll('input[type="checkbox"][data-clip-id]')).toHaveLength(2)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})
it('trims linked heads through controls and restores the source phase with Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.timeUs = 3250000
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const selected = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    selected.checked = true; selected.dispatchEvent(new Event('change', { bubbles: true }))
    const delta = target.querySelector<HTMLInputElement>('[aria-label="Сдвиг левого края клипов"]')!
    delta.value = '0.75'; delta.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Изменить левый край')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 2750000, timelineDurationTick: 2250000, sourceTimelineOffsetTick: 750000 } })
    expect(nestedComposition.timeUs).toBe(3250000)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})

it('replaces the linked AV source from the library with native validation and one Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  library.library = [{ id: 'replacement-source', kind: 'source', filename: 'replacement.mov', url: '/files/sources/replacement.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le', createdAt: 0 }]
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.timeUs = 3250000
  nestedComposition.renderedOutputs = { 'new-timeline': 'stale', root: 'keep' }
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const selected = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    selected.checked = true; selected.dispatchEvent(new Event('change', { bubbles: true }))
    const source = target.querySelector<HTMLSelectElement>('[aria-label="Новый источник выбранных клипов"]')!
    source.value = 'replacement-source'; source.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Заменить источник выбранных клипов')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toEqual({ ...document.compositions['new-timeline']!.content.tracks[index]!.clips[0], sourceId: 'replacement-source' })
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'keep' }); expect(nestedComposition.timeUs).toBe(3250000)
    expect(nestedComposition.document!.compositions['new-timeline']!.content.sources.source).toEqual(document.compositions['new-timeline']!.content.sources.source)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'stale', root: 'keep' })
  } finally { await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})
it('keeps the original and switches the entire linked AV group through version buttons with Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  library.library = [{ id: 'native-copy', kind: 'source', filename: 'copy.mov', url: '/files/sources/copy.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le', createdAt: 0 }]
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const checkbox = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    const source = target.querySelector<HTMLSelectElement>('[aria-label="Новый источник выбранных клипов"]')!
    source.value = 'native-copy'; source.dispatchEvent(new Event('change', { bubbles: true }))
    const keep = target.querySelector<HTMLInputElement>('[aria-label="Сохранить оригинал выбранных клипов"]')!
    keep.checked = true; keep.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = (text: string) => [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === text)!
    button('Заменить источник выбранных клипов').click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
    expect(button('Показать оригинал').disabled).toBe(false)
    button('Показать оригинал').click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2)); await tick()
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toEqual(document.compositions['new-timeline']!.content.tracks[index]!.clips[0])
    expect(button('Показать обработанную копию').disabled).toBe(false)
    button('Показать обработанную копию').click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ sourceId: 'native-copy' })
    undoNestedComposition(); undoNestedComposition(); undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})

it('uploads a local replacement for the captured AV group and restores it with one Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  vi.mocked(api.uploadFile).mockResolvedValueOnce({ id: 'uploaded-native', filename: 'local.mov', url: '/files/sources/local.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le' })
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const override = target.querySelector<HTMLInputElement>('[aria-label="Изменить начало в новом файле"]')!
    override.checked = true; override.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const sourceStart = target.querySelector<HTMLInputElement>('[aria-label="Начало диапазона для замены"]')!
    sourceStart.value = '2'; sourceStart.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    const input = target.querySelector<HTMLInputElement>('[aria-label="Файл для замены выбранных клипов"]')!
    const file = new File(['synthetic'], 'local.mov', { type: 'video/quicktime' })
    Object.defineProperty(input, 'files', { value: [file], configurable: true }); input.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    expect(api.uploadFile).toHaveBeenCalledWith(file)
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ sourceId: 'uploaded-native', placement: { sourceInTick: 2000000, sourceOutTick: 5000000, sourceTimelineOffsetTick: -250000 } })
    expect(input.value).toBe('')
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})
it('retains intervening edits instead of applying a late uploaded replacement', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  let resolve!: (media: Awaited<ReturnType<typeof api.uploadFile>>) => void
  vi.mocked(api.uploadFile).mockReturnValueOnce(new Promise(done => { resolve = done }))
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const input = target.querySelector<HTMLInputElement>('[aria-label="Файл для замены выбранных клипов"]')!
    Object.defineProperty(input, 'files', { value: [new File(['synthetic'], 'late.mov', { type: 'video/quicktime' })], configurable: true })
    input.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(target.querySelector('fieldset')!.disabled).toBe(true)
    const changed = { ...document, compositions: { ...document.compositions, 'new-timeline': { ...document.compositions['new-timeline']!, durationUs: 9000000 } } }
    nestedComposition.document = changed
    resolve({ id: 'late-native', filename: 'late.mov', url: '/files/sources/late.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le' })
    await vi.waitFor(() => expect(nestedHistory.error).toContain('изменился'))
    expect(nestedComposition.document).toEqual(changed); expect(nestedHistory.past).toHaveLength(0)
    await tick(); expect(target.querySelector('fieldset')!.disabled).toBe(false)
  } finally { await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})
it('does not author an uploaded replacement after the controls are unmounted', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  let resolve!: (media: Awaited<ReturnType<typeof api.uploadFile>>) => void
  vi.mocked(api.uploadFile).mockReturnValueOnce(new Promise(done => { resolve = done }))
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  let mounted = true
  try {
    await tick()
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const input = target.querySelector<HTMLInputElement>('[aria-label="Файл для замены выбранных клипов"]')!
    Object.defineProperty(input, 'files', { value: [new File(['synthetic'], 'unmounted.mov', { type: 'video/quicktime' })], configurable: true })
    Object.defineProperty(input, 'value', { value: 'pending-upload', writable: true, configurable: true })
    input.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    await unmount(app); mounted = false
    resolve({ id: 'unmounted-native', filename: 'unmounted.mov', url: '/files/sources/unmounted.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le' })
    await vi.waitFor(() => expect(input.value).toBe('')); await tick()
    expect(nestedComposition.document).toEqual(document); expect(nestedHistory.past).toHaveLength(0)
  } finally { if (mounted) await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})

it('sets a new source in-point for the entire linked replacement through controls', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as api.NativeNestedProject
  const oldLibrary = library.library
  library.library = [{ id: 'shifted-native', kind: 'source', filename: 'shifted.mov', url: '/files/sources/shifted.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le', createdAt: 0 }]
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true }))
    const source = target.querySelector<HTMLSelectElement>('[aria-label="Новый источник выбранных клипов"]')!
    source.value = 'shifted-native'; source.dispatchEvent(new Event('change', { bubbles: true }))
    const override = target.querySelector<HTMLInputElement>('[aria-label="Изменить начало в новом файле"]')!
    override.checked = true; override.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const start = target.querySelector<HTMLInputElement>('[aria-label="Начало диапазона для замены"]')!
    start.value = '2'; start.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Заменить источник выбранных клипов')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ sourceId: 'shifted-native', placement: { sourceInTick: 2000000, sourceOutTick: 5000000, sourceTimelineOffsetTick: -250000 } })
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition(); library.library = oldLibrary }
})

it('duplicates the AV group after the selected span, selects only the new group and restores with Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.nativeVersions[0]!.copy) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.timeUs = 2250000
  nestedComposition.renderedOutputs = { 'new-timeline': 'stale', root: 'keep' }
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Копировать после выбранных')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
    const node = nestedComposition.document!.compositions['new-timeline']!
    for (const index of [1, 3]) expect(node.content.tracks[index]!.clips[1]).toMatchObject({ placement: { timelineStartTick: 5000000, sourceInTick: 1000000, sourceOutTick: 4000000 } })
    const checkboxes = [...target.querySelectorAll<HTMLInputElement>('input[data-clip-id]')]
    expect(checkboxes).toHaveLength(4)
    expect(checkboxes.filter(input => input.checked)).toHaveLength(2)
    expect(target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!.checked).toBe(false)
    expect(Object.keys(node.content.metadata!.clipLinks!)).toHaveLength(2)
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'keep' }); expect(nestedComposition.timeUs).toBe(2250000)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})

it('copies a group to a chosen time and labels original/copy with readable names and ranges', async () => {
  closeNestedComposition()
  const value = JSON.parse(JSON.stringify(fixture.nativeVersions[0]!.copy))
  value.compositions['new-timeline'].content.metadata.clipNames = { 'auto-first': 'Камера A', 'auto-first-audio': 'Звук A' }
  const document = value as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedClipTransfer, { target })
  try {
    await tick()
    expect(target.textContent).toContain('Камера A · 2.000–5.000 с')
    const choose = target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    expect(choose.getAttribute('aria-label')).toBeNull()
    choose.checked = true; choose.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const input = target.querySelector<HTMLInputElement>('[aria-label="Начало копии клипов"]')!
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Копировать в заданное время')!
    input.value = '2.5'; input.dispatchEvent(new Event('input', { bubbles: true })); await tick(); expect(button.disabled).toBe(false);button.click()
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
    for(const index of [1,3])expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[1]).toMatchObject({placement:{timelineStartTick:5000000}})
    undoNestedComposition();expect(nestedComposition.document).toEqual(document);await tick()
    const original=target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!;original.checked=true;original.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    input.value = '8'; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
    for (const index of [1, 3]) expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[index]!.clips[1]).toMatchObject({ placement: { timelineStartTick: 8000000 } })
    expect(target.textContent).toContain('Камера A · 8.000–11.000 с')
    expect(target.textContent).not.toContain('auto-first')
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})

it('copies linked clips into another timeline in one Undo and retains the original timeline', async () => {
  closeNestedComposition()
  const fixture = await import('../../../../../backend/tests/fixtures/concat-merge-preview.json')
  const test=fixture.default.nativeCrossCopies[13]!
  const document=structuredClone(test.before) as unknown as api.NativeNestedProject
  if (document.timelineOrder && !document.timelineOrder.includes(test.to)) document.timelineOrder=[...document.timelineOrder,test.to]
  nestedComposition.document=document; nestedComposition.activeCompositionId=test.from; nestedComposition.timeUs=750000
  nestedComposition.renderedOutputs={ [test.from]:'source-output', [test.to]:'destination-output' }
  vi.mocked(api.moveNativeNestedClips).mockImplementation(async (staged,moves) => {
    const scratch=staged.compositions[moves[0]!.fromComposition]!
    const content={...scratch.content,tracks:scratch.content.tracks.filter(track=>track.clips.length).map(track=>({...track,id:moves.find(move=>move.fromTrackId===track.id)!.toTrackId,clips:track.clips.map(clip=>({...clip,...('placement' in clip?{placement:{...clip.placement,timelineStartTick:8000000}}:{timelineStartTick:8000000})}))}))} as typeof scratch.content
    return { project:{...staged,compositions:{...staged.compositions,[test.to]:{instances:[],content}}},changed:true,affectedCompositions:[moves[0]!.fromComposition,test.to],movedClipIds:moves.map(move=>move.clipId) }
  })
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  try {
    await tick()
    const checkbox=target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))
    const destination=target.querySelector<HTMLSelectElement>('[aria-label="Таймлайн для переноса клипов"]')!
    destination.value=test.to;destination.dispatchEvent(new Event('change',{bubbles:true}))
    const start=target.querySelector<HTMLInputElement>('[aria-label="Начало переноса клипов"]')!
    start.value='8';start.dispatchEvent(new Event('input',{bubbles:true}));await tick()
    const button=[...target.querySelectorAll('button')].find(button=>button.textContent==='Копировать в выбранный таймлайн')!
    expect(button.disabled).toBe(false);button.click()
    await vi.waitFor(()=>expect(nestedComposition.activeCompositionId).toBe(test.to))
    expect(nestedComposition.document!.compositions[test.from]).toEqual(document.compositions[test.from])
    expect(Object.keys(nestedComposition.document!.compositions).sort()).toEqual(Object.keys(document.compositions).sort())
    expect(nestedComposition.renderedOutputs[test.from]).toBe('source-output')
    expect(nestedComposition.renderedOutputs[test.to]).toBeUndefined()
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition();expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.timeUs).toBe(750000)
  } finally {await unmount(component);target.remove();closeNestedComposition()}
})


it('copies to a session buffer, deletes originals and pastes their saved content with one Undo',async()=>{
  closeNestedComposition();nestedClipboard.value=null
  const document=structuredClone(fixture.nativeVersions[0]!.copy) as unknown as api.NativeNestedProject
  nestedComposition.document=document;nestedComposition.activeCompositionId='new-timeline'
  vi.mocked(api.moveNativeNestedClips).mockImplementation(async (staged,moves)=>{
    const scratch=staged.compositions[moves[0]!.fromComposition]!
    const node={...scratch,content:{...scratch.content,tracks:scratch.content.tracks.map(track=>({...track,clips:track.clips.map(clip=>{const move=moves.find(move=>move.clipId===clip.id);return move && 'placement' in clip?{...clip,placement:{...clip.placement,timelineStartTick:move.startUs}}:clip})}))}} as typeof scratch
    return {project:{...staged,compositions:{...staged.compositions,'new-timeline':node}},changed:true,affectedCompositions:['new-timeline'],movedClipIds:moves.map(move=>move.clipId)}
  })
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try {
    await tick()
    const checkbox=target.querySelector<HTMLInputElement>('[data-clip-id="auto-first"]')!
    checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    button('Копировать в буфер').click();await vi.waitFor(()=>expect(nestedClipboard.value).not.toBeNull())
    expect(nestedHistory.past).toHaveLength(0);const captured=JSON.stringify(nestedClipboard.value)
    button('Удалить выбранные клипы').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
    const deleted=JSON.parse(JSON.stringify(nestedComposition.document))
    const destination=target.querySelector<HTMLSelectElement>('[aria-label="Таймлайн для переноса клипов"]')!
    destination.value='new-timeline';destination.dispatchEvent(new Event('change',{bubbles:true}))
    const start=target.querySelector<HTMLInputElement>('[aria-label="Начало переноса клипов"]')!
    start.value='8';start.dispatchEvent(new Event('input',{bubbles:true}));await tick()
    expect(button('Вставить из буфера').disabled).toBe(false);button('Вставить из буфера').click()
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
    const clips=nestedComposition.document!.compositions['new-timeline']!.content.tracks.flatMap(track=>track.clips.map(clip=>clip))
    expect(clips).toHaveLength(2);expect(clips.every(clip=>'placement' in clip && clip.placement.timelineStartTick===8000000)).toBe(true)
    expect(clips.some(clip=>clip.id==='auto-first')).toBe(false)
    expect(JSON.stringify(nestedClipboard.value)).toBe(captured)
    undoNestedComposition();expect(nestedComposition.document).toEqual(deleted);expect(nestedClipboard.value).not.toBeNull()
    undoNestedComposition();expect(nestedComposition.document).toEqual(document)
    await tick();button('Очистить буфер клипов').click();await tick();expect(nestedClipboard.value).toBeNull()
  } finally {await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null}
})

it('captures caption-only selection, keeps its saved text after an edit and pastes precise cues with one Undo',async()=>{
  closeNestedComposition();nestedClipboard.value=null
  const document=structuredClone(fixture.nativeVersions[0]!.copy) as unknown as api.NativeNestedProject
  const node=document.compositions['new-timeline']!
  document.compositions['new-timeline']={...node,content:{...node.content,captionTracks:[{id:'captions',name:'Титры',style:{font:'Inter',size:8,background:false},clips:[{id:'caption-original',timelineStartTick:String(1000000n*254016n+1n),timelineEndTick:String(2000000n*254016n+2n),text:'Сохранённый текст'}]}]}}
  nestedComposition.document=document;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=500000
  vi.mocked(api.moveNativeNestedClips).mockClear()
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try {
    await tick()
    const checkbox=target.querySelector<HTMLInputElement>('[data-caption-id="caption-original"]')!
    checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    expect(button('Копировать в буфер').disabled).toBe(false);button('Копировать в буфер').click()
    await vi.waitFor(()=>expect(nestedClipboard.value).not.toBeNull());expect(nestedHistory.past).toHaveLength(0)
    const original=nestedComposition.document!, current=original.compositions['new-timeline']!
    const edited={...original,compositions:{...original.compositions,'new-timeline':{...current,content:{...current.content,captionTracks:current.content.captionTracks!.map(track=>({...track,clips:track.clips.map(cue=>({...cue,text:'Изменённый оригинал'}))}))}}}}
    nestedComposition.document=edited
    const destination=target.querySelector<HTMLSelectElement>('[aria-label="Таймлайн для переноса клипов"]')!
    destination.value='new-timeline';destination.dispatchEvent(new Event('change',{bubbles:true}))
    const start=target.querySelector<HTMLInputElement>('[aria-label="Начало переноса клипов"]')!
    start.value='8';start.dispatchEvent(new Event('input',{bubbles:true}));await tick()
    expect(button('Вставить из буфера').disabled).toBe(false);button('Вставить из буфера').click()
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
    const tracks=nestedComposition.document!.compositions['new-timeline']!.content.captionTracks!
    expect(tracks).toHaveLength(2)
    expect(tracks[1]!.clips[0]).toMatchObject({text:'Сохранённый текст',timelineStartTick:String(8000000n*254016n+1n),timelineEndTick:String(9000000n*254016n+2n)})
    expect(api.moveNativeNestedClips).not.toHaveBeenCalled()
    undoNestedComposition();expect(nestedComposition.document).toEqual(edited);expect(nestedComposition.timeUs).toBe(500000)
  } finally {await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null}
})


it('edits imported caption appearance and wrap width on one track with undo', async () => {
  closeNestedComposition()
  const project = structuredClone(fixture.nativeVersions[0]!.copy) as unknown as api.NativeNestedProject
  const compositionId = 'new-timeline'
  const node = project.compositions[compositionId]!
  node.content = { ...node.content, captionTracks: [
    { id: 'speech', name: 'Распознанная речь', style: { wrapWidth: 0.84, margin: 0.08 }, clips: [] },
    { id: 'locked', name: 'Защищённая', locked: true, clips: [] },
  ] }
  nestedComposition.document = project
  nestedComposition.activeCompositionId = compositionId
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(NestedClipTransfer, { target })
  try {
    await tick()
    const width = target.querySelector<HTMLInputElement>('[aria-label="Ширина переноса субтитров: Распознанная речь"]')!
    expect(width.value).toBe('84')
    expect(target.querySelector<HTMLInputElement>('[aria-label="Ширина переноса субтитров: Защищённая"]')!.disabled).toBe(true)
    width.value = '75'
    width.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    expect(nestedComposition.document?.compositions[compositionId]?.content.captionTracks?.[0]?.style)
      .toEqual({ wrapWidth: 0.75, margin: 0.08 })
    width.value = ''
    width.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
    expect(nestedComposition.document?.compositions[compositionId]?.content.captionTracks?.[0]?.style)
      .toEqual({ margin: 0.08 })
    const appearance = target.querySelector('details:has([aria-label="Ширина переноса субтитров: Распознанная речь"])')!
    const anchor = appearance.querySelector<HTMLSelectElement>('[aria-label="Положение субтитров"]')!
    anchor.value = 'top'
    anchor.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
    expect(nestedComposition.document?.compositions[compositionId]?.content.captionTracks?.[0]?.style)
      .toEqual({ margin: 0.08, anchor: 'top' })
    expect(nestedComposition.document?.compositions[compositionId]?.content.captionTracks?.[1])
      .toEqual(node.content.captionTracks?.[1])
    undoNestedComposition()
    undoNestedComposition()
    undoNestedComposition()
    expect(nestedComposition.document).toEqual(project)
  } finally {
    await unmount(component)
    target.remove()
    closeNestedComposition()
  }
})

it('cuts selected captions into the buffer through the button and restores originals with one Undo',async()=>{
  closeNestedComposition();nestedClipboard.value=null
  const document=structuredClone(fixture.nativeCaptionPastes[0]!.reference) as unknown as api.NativeNestedProject
  nestedComposition.document=document;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=2250000
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  try {
    await tick()
    for (const id of ['cue-first','cue-second']) {const checkbox=target.querySelector<HTMLInputElement>(`[data-caption-id="${id}"]`)!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))}
    await tick()
    const cut=[...target.querySelectorAll('button')].find(button=>button.textContent==='Вырезать в буфер')!
    expect(cut.disabled).toBe(false);cut.click()
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
    expect(nestedComposition.document!.compositions['new-timeline']!.content.captionTracks![0]!.clips).toEqual([])
    expect(nestedClipboard.value!.compositions.clipboard!.content.captionTracks![0]!.clips).toHaveLength(2)
    expect(nestedComposition.timeUs).toBe(2250000)
    undoNestedComposition();expect(nestedComposition.document).toEqual(document)
    expect(nestedClipboard.value).not.toBeNull()
  } finally {await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null}
})


it('uses scoped configurable clipboard keys while leaving text editing alone and pasting at the active cursor',async()=>{
  closeNestedComposition();nestedClipboard.value=null
  const bindings={...shortcutState.bindings};shortcutState.bindings={...bindings,'native.copy':'Alt+KeyY'}
  const document=structuredClone(fixture.nativeCaptionPastes[0]!.reference) as unknown as api.NativeNestedProject
  nestedComposition.document=document;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=8000000
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  const key=(element:Element,code:string,options:KeyboardEventInit={})=>{const event=new KeyboardEvent('keydown',{code,key:code,bubbles:true,cancelable:true,...options});element.dispatchEvent(event);return event}
  try {
    await tick()
    const checkbox=target.querySelector<HTMLInputElement>('[data-caption-id="cue-first"]')!
    checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    const number=target.querySelector<HTMLInputElement>('[aria-label="Начало переноса клипов"]')!
    number.value='99';number.dispatchEvent(new Event('input',{bubbles:true}));await tick()
    expect(key(number,'KeyY',{altKey:true}).defaultPrevented).toBe(false);expect(nestedClipboard.value).toBeNull()
    expect(key(checkbox,'KeyY',{altKey:true,isComposing:true}).defaultPrevented).toBe(false)
    expect(key(checkbox,'KeyC',{ctrlKey:true}).defaultPrevented).toBe(false)
    checkbox.focus()
    expect(key(checkbox,'KeyY',{key:'н',altKey:true}).defaultPrevented).toBe(true)
    await vi.waitFor(()=>expect(nestedClipboard.value).not.toBeNull());expect(nestedHistory.past).toHaveLength(0)
    expect(key(checkbox,'KeyX',{ctrlKey:true}).defaultPrevented).toBe(true)
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
    const button=target.querySelector<HTMLFieldSetElement>('[data-native-clipboard]')!
    expect(window.document.activeElement).toBe(button)
    expect(key(button,'KeyV',{ctrlKey:true,repeat:true}).defaultPrevented).toBe(true);expect(nestedHistory.past).toHaveLength(1)
    expect(key(button,'KeyV',{key:'м',metaKey:true}).defaultPrevented).toBe(true)
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
    const pasted=nestedComposition.document!.compositions['new-timeline']!.content.captionTracks![1]!.clips[0]!
    expect(pasted.timelineStartTick).toBe(String(8000000n*254016n+1n))
    key(button,'KeyZ',{ctrlKey:true});expect(nestedHistory.past).toHaveLength(1)
    key(button,'KeyZ',{ctrlKey:true,shiftKey:true});expect(nestedHistory.past).toHaveLength(2);await tick()
    const pastedCheckbox=target.querySelector<HTMLInputElement>(`[data-caption-id="${pasted.id}"]`)!
    pastedCheckbox.checked=true;pastedCheckbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
    key(pastedCheckbox,'Delete');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
    expect(nestedComposition.document!.compositions['new-timeline']!.content.captionTracks![1]!.clips).toEqual([])
  } finally {shortcutState.bindings=bindings;await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null}
})

it('removes its capture keyboard listener when the native clip panel unmounts',async()=>{
  closeNestedComposition()
  nestedComposition.document=structuredClone(fixture.nativeCaptionPastes[0]!.reference) as unknown as api.NativeNestedProject
  nestedComposition.activeCompositionId='new-timeline'
  const add=vi.spyOn(window,'addEventListener'), remove=vi.spyOn(window,'removeEventListener')
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  try {
    await tick()
    const registration=add.mock.calls.find(call=>call[0]==='keydown' && call[2]===true)!
    expect(registration).toBeDefined()
    await unmount(component)
    expect(remove).toHaveBeenCalledWith('keydown',registration[1],true)
  } finally {add.mockRestore();remove.mockRestore();target.remove();closeNestedComposition()}
})


it('exports the session buffer to the OS and validates a read before replacing it, without authoring',async()=>{
  closeNestedComposition()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.reference) as unknown as api.NativeNestedProject
  const incoming=fixture.nativeCaptionPastes[1]!.clipboard as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  nestedClipboard.value=structuredClone(fixture.nativeClipboardPastes[0]!.clipboard) as unknown as api.NativeNestedProject
  const exported=JSON.stringify(nestedClipboard.value),descriptor=Object.getOwnPropertyDescriptor(navigator,'clipboard')
  const writeText=vi.fn().mockResolvedValue(undefined),readText=vi.fn().mockResolvedValue(serializeNativeClipboard(incoming))
  Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText,readText}})
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target})
  const button=(text:string)=>[...target.querySelectorAll('button')].find(button=>button.textContent===text)!
  try {
    await tick();button('Передать буфер другому окну').click()
    await vi.waitFor(()=>expect(writeText).toHaveBeenCalledTimes(1))
    expect(JSON.stringify(parseNativeClipboard(writeText.mock.calls[0]![0]))).toBe(exported)
    await vi.waitFor(()=>expect(target.textContent).toContain('Клипы скопированы'))
    button('Прочитать системный буфер').click()
    await vi.waitFor(()=>expect(nestedClipboard.value).toEqual(incoming))
    expect(api.evaluateNestedFrame).toHaveBeenCalledWith(incoming,0)
    expect(nestedComposition.document).toEqual(original);expect(nestedHistory.past).toHaveLength(0)
    await tick();readText.mockRejectedValueOnce(new DOMException('Denied','NotAllowedError'))
    button('Прочитать системный буфер').click()
    await vi.waitFor(()=>expect(target.querySelector('[role="alert"]')?.textContent).toContain('не разрешил доступ'))
    expect(nestedClipboard.value).toEqual(incoming);expect(nestedComposition.document).toEqual(original)
  } finally {await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null;if(descriptor)Object.defineProperty(navigator,'clipboard',descriptor);else Reflect.deleteProperty(navigator,'clipboard')}
})
it('ignores a late OS clipboard read after the clip panel unmounts',async()=>{
  closeNestedComposition()
  const original=structuredClone(fixture.nativeCaptionPastes[0]!.reference) as unknown as api.NativeNestedProject
  nestedComposition.document=original;nestedComposition.activeCompositionId='new-timeline'
  nestedClipboard.value=structuredClone(fixture.nativeClipboardPastes[0]!.clipboard) as unknown as api.NativeNestedProject
  const saved=JSON.stringify(nestedClipboard.value),descriptor=Object.getOwnPropertyDescriptor(navigator,'clipboard')
  let release!:(value:string)=>void
  Object.defineProperty(navigator,'clipboard',{configurable:true,value:{readText:vi.fn(()=>new Promise<string>(resolve=>{release=resolve}))}})
  const target=window.document.createElement('div');window.document.body.append(target)
  const component=mount(NestedClipTransfer,{target});let mounted=true
  try {
    await tick();[...target.querySelectorAll('button')].find(button=>button.textContent==='Прочитать системный буфер')!.click()
    await vi.waitFor(()=>expect(release).toBeDefined())
    await unmount(component);mounted=false
    release(serializeNativeClipboard(fixture.nativeCaptionPastes[1]!.clipboard as unknown as api.NativeNestedProject))
    await new Promise(resolve=>setTimeout(resolve,0));expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
    expect(nestedComposition.document).toEqual(original);expect(nestedHistory.past).toHaveLength(0)
  } finally {if(mounted)await unmount(component);target.remove();closeNestedComposition();nestedClipboard.value=null;if(descriptor)Object.defineProperty(navigator,'clipboard',descriptor);else Reflect.deleteProperty(navigator,'clipboard')}
})

it('closes a gap and ripple-deletes selected clips through actual native controls with Undo',async()=>{
 closeNestedComposition();agentSession.lock=null
 let authored=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'})
 authored=addTrack(authored,{id:'titles',kind:'text',name:'Titles',hidden:false,locked:false,clips:[]})
 for(const [id,start] of [['a',0],['b',2000000]] as const)authored=addClip(authored,'titles',{id,kind:'text',timelineStartTicks:start,durationTicks:1000000,text:id,x:0,y:0,opacity:1,style:{fontSizePx:12,color:'#FFFFFFFF',align:'center'}})
 const original:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(authored),instances:[]}}}
 nestedComposition.document=original;nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1500000
 vi.mocked(api.moveNativeNestedClips).mockImplementation(async(project,moves)=>{
  const output=structuredClone(project)
  for(const move of moves){const clip=output.compositions.root!.content.tracks[0]!.clips.find(clip=>clip.id===move.clipId)!;Object.assign(clip,{timelineStartTick:move.startUs,timelineEndTick:move.startUs+1000000})}
  return {project:output,changed:true,affectedCompositions:['root'],movedClipIds:moves.map(move=>move.clipId)}
 })
 const target=window.document.createElement('div');window.document.body.append(target)
 const component=mount(NestedClipTransfer,{target})
 const button=(text:string)=>[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent===text)!
 try{
  await tick();expect(button('Закрыть промежуток на курсоре').disabled).toBe(false);button('Закрыть промежуток на курсоре').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[1]).toMatchObject({timelineStartTick:1000000}))
  expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();await tick();expect(nestedComposition.document).toEqual(original)
  const checkbox=target.querySelector<HTMLInputElement>('[data-clip-id="a"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  button('Удалить выбранные со сдвигом').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips).toHaveLength(1))
  expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({id:'b',timelineStartTick:1000000})
  undoNestedComposition();await tick();expect(nestedComposition.document).toEqual(original)
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick();expect(button('Закрыть промежуток на курсоре').disabled).toBe(true)
 }finally{await unmount(component);target.remove();agentSession.lock=null;closeNestedComposition()}
})
it('raises selected layers through actual controls preserving clocks or landing at the cursor',async()=>{
 closeNestedComposition();agentSession.lock=null
 let authored=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'})
 authored=addTrack(authored,{id:'titles',kind:'text',name:'Titles',hidden:false,locked:false,clips:[]})
 for(const [id,start] of [['a',0],['b',2000000]] as const)authored=addClip(authored,'titles',{id,kind:'text',timelineStartTicks:start,durationTicks:1000000,text:id,x:0,y:0,opacity:1,style:{fontSizePx:12,color:'#FFFFFFFF',align:'center'}})
 const original:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(authored),instances:[]}}}
 nestedComposition.document=original;nestedComposition.activeCompositionId='root';nestedComposition.timeUs=3000000
 vi.mocked(api.moveNativeNestedClips).mockImplementation(async(project,moves)=>{
  const output=structuredClone(project),tracks=output.compositions.root!.content.tracks
  for(const move of moves){const from=tracks.find(track=>track.id===move.fromTrackId)!,to=tracks.find(track=>track.id===move.toTrackId)!,clip=from.clips.find(clip=>clip.id===move.clipId)!;Object.assign(from,{clips:from.clips.filter(clip=>clip.id!==move.clipId)});Object.assign(to,{clips:[...to.clips,{...clip,timelineStartTick:move.startUs,timelineEndTick:move.startUs+1000000}]})}
  return {project:output,changed:true,affectedCompositions:['root'],movedClipIds:moves.map(move=>move.clipId)}
 })
 const target=window.document.createElement('div');window.document.body.append(target)
 const component=mount(NestedClipTransfer,{target})
 const button=(text:string)=>[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent===text)!
 try{
  await tick();expect(button('На новую дорожку с сохранением времени').disabled).toBe(true)
  for(const id of ['a','b']){const checkbox=target.querySelector<HTMLInputElement>(`[data-clip-id="${id}"]`)!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
  button('На новую дорожку с сохранением времени').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.id).not.toBe('titles'))
  expect(nestedComposition.document!.compositions.root!.content.tracks).toHaveLength(1)
  expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips.map(clip=>'timelineStartTick' in clip?clip.timelineStartTick:0)).toEqual([0,2000000])
  expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();await tick();expect(nestedComposition.document).toEqual(original)
  button('На новую дорожку на курсор').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.id).not.toBe('titles'))
  expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips.map(clip=>'timelineStartTick' in clip?clip.timelineStartTick:0)).toEqual([3000000,5000000])
  undoNestedComposition();await tick();expect(nestedComposition.document).toEqual(original)
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick();expect(button('На новую дорожку на курсор').disabled).toBe(true)
 }finally{await unmount(component);target.remove();agentSession.lock=null;closeNestedComposition()}
})

it('slips source material through native controls without moving the clip and supports Undo',async()=>{
 closeNestedComposition();agentSession.lock=null
 let authored=registerSource(createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}),{id:'source',kind:'video',durationTicks:4000000,width:16,height:16,hasAudio:false})
 authored=addTrack(authored,{id:'v',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[]})
 authored=addClip(authored,'v',{id:'picture',kind:'video',sourceId:'source',timelineStartTicks:1000000,sourceInTicks:0,sourceOutTicks:2000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1})
 const original:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(authored),instances:[]}}}
 nestedComposition.document=original;nestedComposition.activeCompositionId='root';vi.mocked(api.evaluateNestedFrame).mockResolvedValue({entries:[]})
 const target=window.document.createElement('div');window.document.body.append(target)
 const component=mount(NestedClipTransfer,{target})
 try{
  await tick();const checkbox=target.querySelector<HTMLInputElement>('[data-clip-id="picture"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  const input=target.querySelector<HTMLInputElement>('[aria-label="Сдвиг материала внутри клипа"]')!;input.value='0.5';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
  const button=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Сдвинуть материал внутри клипа')!;button.click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({placement:{sourceInTick:500000,sourceOutTick:2500000,timelineStartTick:1000000}}))
  expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).toEqual(original)
 }finally{await unmount(component);target.remove();closeNestedComposition()}
})
