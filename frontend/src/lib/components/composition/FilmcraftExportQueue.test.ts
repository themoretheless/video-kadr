// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { downloadFilmcraftVideo, getFilmcraftVideoJob, editFilmcraftExportQueue, listFilmcraftExportQueue, type FilmcraftExportQueue as Queue } from '$lib/api'
import FilmcraftExportQueue from './FilmcraftExportQueue.svelte'
vi.mock('$lib/api', () => ({ listFilmcraftExportQueue: vi.fn(), editFilmcraftExportQueue: vi.fn(), getFilmcraftVideoJob: vi.fn(), downloadFilmcraftVideo: vi.fn() }))
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


it('downloads a completed queue output and immediately starts a retry', async () => {
  const done: Queue = { running: false, items: [{ ...initial.items[0]!, status: 'done', progress: 1 }] }
  const output = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(listFilmcraftExportQueue).mockResolvedValue(done)
  vi.mocked(editFilmcraftExportQueue).mockResolvedValue(done)
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'worker', status: 'done', result: { id: output } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['wav']))
  const url = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:queued')
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
    expect(this.download).toBe(`${output}.wav`)
  })
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  try {
    component = mount(FilmcraftExportQueue, { target: document.body, props: { token: 'owner', input: () => ({ document: '{}', sequenceId: '6', sources: {} }) } })
    const button = (label: string) => [...document.querySelectorAll('button')].find(item => item.textContent?.trim() === label)
    await vi.waitFor(() => expect(button('Скачать WAV')).toBeDefined())
    button('Скачать WAV')!.click()
    await vi.waitFor(() => expect(click).toHaveBeenCalled())
    expect(getFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'worker', expect.any(AbortSignal))
    expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', output, expect.any(AbortSignal), 'wav')
    button('Повторить')!.click()
    await vi.waitFor(() => expect(editFilmcraftExportQueue).toHaveBeenCalledWith('owner', { action: 'retry', id: '9', start: true }, expect.any(AbortSignal)))
    await unmount(component); component = undefined
    expect(revoke).toHaveBeenCalledWith('blob:queued')
  } finally { url.mockRestore(); click.mockRestore(); revoke.mockRestore() }
})
