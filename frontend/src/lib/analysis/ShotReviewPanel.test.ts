// @ts-expect-error Tests need Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from '../composition/commands.js'
import type { VideoClip } from '../composition/types.js'
import { authState } from '../state/auth.svelte.js'
import { compositionState, resetCompositionForTests, selectCompositionClip } from '../state/composition/core.svelte.js'
import { spacesState } from '../state/spaces.svelte.js'
import { shotAnalysisState } from './shotState.svelte.js'
import ShotReviewPanel from './ShotReviewPanel.svelte'

it('describes a reverse trimmed clip in source time and cancels on selection change', async () => {
  resetCompositionForTests()
  const sourceHash = 'a'.repeat(64)
  const previousToken = authState.token
  const previousSpace = spacesState.selectedId
  authState.token = 'owner'
  spacesState.selectedId = 'timeline-space'
  let documentModel = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  documentModel = registerSource(documentModel, { id: 'timeline-source', kind: 'video', durationTicks: 4_000_000, width: 1280, height: 720, hasAudio: false })
  documentModel = addTrack(documentModel, { id: 'video', name: 'Video', kind: 'video', locked: false, hidden: false, muted: false, transitions: [], clips: [] })
  const clip: VideoClip = {
    id: 'reverse', kind: 'video', sourceId: 'timeline-source', sourceInTicks: 500_000,
    sourceOutTicks: 3_500_000, timelineStartTicks: 5_000_000, opacity: 1,
    transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
    sourceAudioEnabled: false, audioGain: 1, playbackMode: { mode: 'reverse' }, speed: 2,
  }
  documentModel = addClip(documentModel, 'video', clip)
  compositionState.document = addClip(documentModel, 'video', {
    ...clip, id: 'still', timelineStartTicks: 8_000_000, playbackMode: { mode: 'freeze', sourceTick: 2_500_000 },
  })
  selectCompositionClip('video', 'reverse', { single: true })
  shotAnalysisState.scans['timeline-source'] = {
    version: 1, sourceHash, durationTicks: 4_000_000, floor: 0.05, cuts: [{ tick: 2_000_000, score: 0.8 }],
  }
  let finishScene!: (response: Response) => void
  const delayed = new Promise<Response>(resolve => { finishScene = resolve })
  const fetchMock = vi.fn<(url: string, init?: RequestInit) => Promise<Response>>()
    .mockImplementation(async () => Response.json({ text: 'Стоп-кадр', tags: [] }))
    .mockResolvedValueOnce(Response.json({ text: 'Весь диапазон', tags: ['общий план'] }))
    .mockReturnValueOnce(delayed)
  vi.stubGlobal('fetch', (url: string, init?: RequestInit) =>
    url.endsWith('/shots/descriptions') ? Promise.resolve(Response.json([])) : fetchMock(url, init))
  const target = document.createElement('div')
  document.body.append(target)
  const panel = mount(ShotReviewPanel, { target })
  const button = (label: string) => [...target.querySelectorAll('button')].find(item => item.textContent?.trim() === label)!
  const request = (index: number) => JSON.parse(String(fetchMock.mock.calls[index]?.[1]?.body))
  const before = JSON.stringify(compositionState.document)
  try {
    await tick()
    expect(fetchMock).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(button('Описать весь клип').disabled).toBe(false))
    button('Описать весь клип').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Весь диапазон'))
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/timeline-source/shots/describe')
    expect(new Headers(fetchMock.mock.calls[0]?.[1]?.headers).get('x-space-id')).toBe('timeline-space')
    expect(request(0)).toEqual({
      span: { startTicks: 500_000, endTicks: 3_500_000 },
      expectedSourceHash: sourceHash, focus: 'general', language: 'ru',
    })

    button('Описать все сцены (2)').click()
    await tick()
    expect(request(1).span).toEqual({ startTicks: 2_000_000, endTicks: 3_500_000 })
    expect(target.textContent).toContain('Модель описывает кадры')
    const signal = fetchMock.mock.calls[1]?.[1]?.signal
    selectCompositionClip('video', 'still', { single: true })
    await tick()
    expect(signal?.aborted).toBe(true)
    finishScene(Response.json({ text: 'Описание другого клипа', tags: [] }))
    await vi.waitFor(() => expect(button('Описать клип').disabled).toBe(false))
    button('Описать клип').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Стоп-кадр'))
    expect(request(2).span).toEqual({ startTicks: 2_500_000, endTicks: 2_500_001 })
    expect(target.textContent).not.toContain('Описание другого клипа')
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(0)
  } finally {
    await unmount(panel)
    target.remove()
    vi.unstubAllGlobals()
    delete shotAnalysisState.scans['timeline-source']
    resetCompositionForTests()
    authState.token = previousToken
    spacesState.selectedId = previousSpace
  }
})
