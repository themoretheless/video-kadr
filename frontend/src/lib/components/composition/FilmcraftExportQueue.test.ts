// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftExportQueue, listFilmcraftExportQueue, type FilmcraftExportQueue as Queue } from '$lib/api'
import FilmcraftExportQueue from './FilmcraftExportQueue.svelte'
vi.mock('$lib/api', () => ({ listFilmcraftExportQueue: vi.fn(), editFilmcraftExportQueue: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
const initial: Queue = { running: true, items: [{ id: '9', sequenceId: '6', sequenceName: 'Scene', preset: 'Wave', container: 'wav', status: 'encoding', jobId: 'worker', error: null, progress: 0.25, statusText: 'Encoding', etaSeconds: 61.2, etaText: '1:02', sourceRange: null }] }
it('keeps the active item while stopping and aborts observation on unmount', async () => {
  vi.mocked(listFilmcraftExportQueue).mockResolvedValue(initial)
  vi.mocked(editFilmcraftExportQueue).mockResolvedValue({ ...initial, running: false })
  component = mount(FilmcraftExportQueue, { target: document.body, props: { token: 'owner', input: () => ({ document: '{}', sequenceId: '6', sources: {}, startFrames: '0', frames: 2, preset: 'lossless' }) } })
  await vi.waitFor(() => expect(document.querySelector('article')).not.toBeNull())
  const button = (label: string) => [...document.querySelectorAll('button')].find(item => item.textContent === label)!
  expect(button('Убрать').disabled).toBe(true)
  expect(document.querySelector('progress')?.value).toBe(0.25)
  expect(document.body.textContent).toContain('Осталось: 1:02')
  button('Остановить после текущего').click()
  await vi.waitFor(() => expect(editFilmcraftExportQueue).toHaveBeenCalledWith('owner', { action: 'stop' }, expect.any(AbortSignal)))
  await tick()
  expect(document.querySelector('article')?.textContent).toContain('Экспортируется')
  const signal = vi.mocked(listFilmcraftExportQueue).mock.calls[0]![1]!
  await unmount(component!); component = undefined
  expect(signal.aborted).toBe(true)
})
it('adds the exact current settings without starting a job automatically', async () => {
  vi.mocked(listFilmcraftExportQueue).mockResolvedValue({ running: false, items: [] })
  vi.mocked(editFilmcraftExportQueue).mockResolvedValue({ running: false, items: [] })
  const input = { document: '{"snapshot":true}', sequenceId: '6', sources: {}, startFrames: '9007199254740993', frames: 2, preset: 'lossless' as const }
  component = mount(FilmcraftExportQueue, { target: document.body, props: { token: 'owner', input: () => input } })
  await tick()
  ;[...document.querySelectorAll('button')].find(item => item.textContent === 'Добавить в очередь')!.click()
  await vi.waitFor(() => expect(editFilmcraftExportQueue).toHaveBeenCalledWith('owner', { action: 'add', input }, expect.any(AbortSignal)))
})

it('does not let an older poll erase a newly accepted queue item', async () => {
  let resolvePoll!: (value: Queue) => void
  vi.mocked(listFilmcraftExportQueue).mockImplementation(() => new Promise(resolve => { resolvePoll = resolve }))
  vi.mocked(editFilmcraftExportQueue).mockResolvedValue({ running: false, items: [{ ...initial.items[0]!, status: 'ready', jobId: null }] })
  component = mount(FilmcraftExportQueue, { target: document.body, props: { token: 'owner', input: () => ({ document: '{}', sequenceId: '6', sources: {}, startFrames: '0', frames: 2 }) } })
  await tick()
  ;[...document.querySelectorAll('button')].find(item => item.textContent === 'Добавить в очередь')!.click()
  await vi.waitFor(() => expect(document.querySelector('article')?.textContent).toContain('Ожидает'))
  resolvePoll({ running: false, items: [] })
  await tick()
  expect(document.querySelector('article')?.textContent).toContain('Ожидает')
  expect(document.body.textContent).not.toContain('Осталось:')
})
