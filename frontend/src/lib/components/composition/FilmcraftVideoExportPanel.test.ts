// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { listFilmcraftExportQueue, startFilmcraftQuickExportJob, startFilmcraftVideoJob, getFilmcraftVideoJob, cancelFilmcraftVideoJob, retryFilmcraftVideoJob, downloadFilmcraftVideo, listFilmcraftVideoJobs, listFilmcraftExportPresets } from '$lib/api'
import FilmcraftVideoExportPanel from './FilmcraftVideoExportPanel.svelte'
vi.mock('$lib/api', () => ({ startFilmcraftQuickExportJob: vi.fn(), startFilmcraftVideoJob: vi.fn(), getFilmcraftVideoJob: vi.fn(), cancelFilmcraftVideoJob: vi.fn(), retryFilmcraftVideoJob: vi.fn(), downloadFilmcraftVideo: vi.fn(), listFilmcraftExportQueue: vi.fn().mockResolvedValue({ running: false, items: [] }), editFilmcraftExportQueue: vi.fn(), listFilmcraftVideoJobs: vi.fn(), listFilmcraftExportPresets: vi.fn(), editFilmcraftExportPresets: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const id = '12345678-1234-1234-1234-123456789abc'
function button(text: string) { return [...document.querySelectorAll('button')].find(button => button.textContent === text)! }
async function open(token: string | null = 'owner', documentText = '{"original":true}') {
  component = mount(FilmcraftVideoExportPanel, { target: document.body, props: { document: documentText, sequenceId: '18446744073709551615', sources: { '9007199254740993': 'source', unused: '' }, token, initialFrames: '48', imageSources: [{ id: 'logo', kind: 'source', filename: 'logo.png', url: '/logo.png', createdAt: 0, mediaType: 'image' }] } })
  await tick()
}
beforeEach(() => {
  vi.mocked(listFilmcraftExportQueue).mockResolvedValue({ running: false, items: [] })
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [], nextCursor: null })
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'job', mode: 'video_only', container: 'mkv' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'job', status: 'done', result: { id, filename: `${id}.mkv` } })
  vi.mocked(cancelFilmcraftVideoJob).mockResolvedValue({})
})
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.innerHTML = ''; vi.restoreAllMocks(); vi.resetAllMocks() })
it('submits full native frame count with exact identifiers and downloads with authentication', async () => {
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:video')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.mkv`) })
  const blob = new Blob(['mkv']); vi.mocked(downloadFilmcraftVideo).mockResolvedValue(blob)
  await open()
  const input = document.querySelector<HTMLInputElement>('[aria-label="Начальный кадр видеоэкспорта Filmcraft"]')!
  input.value = '9007199254740993'; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', { document: '{"original":true}', sequenceId: '18446744073709551615', startFrames: '9007199254740993', frames: 48, sources: { '9007199254740993': 'source' } }, expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Скачать видео MKV')).toBeDefined())
  expect(getFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'job', expect.any(AbortSignal))
  button('Скачать видео MKV').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal))
  await unmount(component!); component = undefined
  expect(revoke).toHaveBeenCalledWith('blob:video')
})
it('cancels an active job and displays terminal cancellation', async () => {
  vi.mocked(getFilmcraftVideoJob).mockResolvedValueOnce({ id: 'job', status: 'running', progress: 40 }).mockResolvedValue({ id: 'job', status: 'cancelled' })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('progress')?.value).toBe(40))
  button('Отменить видеоэкспорт').click()
  await vi.waitFor(() => expect(cancelFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'job', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(document.body.textContent).toContain('Видеоэкспорт отменён'))
})
it('retains a job after poll failure and allows status recovery', async () => {
  vi.mocked(getFilmcraftVideoJob).mockRejectedValueOnce(new Error('Сеть недоступна')).mockResolvedValue({ id: 'job', status: 'done', result: { id } })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Сеть недоступна'))
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
  button('Проверить видеоэкспорт').click()
  await vi.waitFor(() => expect(button('Скачать видео MKV')).toBeDefined())
  expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(1)
})
it('surfaces cancellation failures and permits retry', async () => {
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'job', status: 'running' })
  vi.mocked(cancelFilmcraftVideoJob).mockRejectedValue(new Error('Отмена недоступна'))
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(button('Отменить видеоэкспорт')).toBeDefined())
  button('Отменить видеоэкспорт').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Отмена недоступна'))
  expect(button('Отменить видеоэкспорт').disabled).toBe(false)
})
it('aborts monitoring on unmount and ignores a late response', async () => {
  let finish!: (value: Awaited<ReturnType<typeof getFilmcraftVideoJob>>) => void
  vi.mocked(getFilmcraftVideoJob).mockReturnValue(new Promise(resolve => { finish = resolve }))
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(getFilmcraftVideoJob).toHaveBeenCalled())
  const signal = vi.mocked(getFilmcraftVideoJob).mock.calls[0][2]!
  await unmount(component!); component = undefined
  expect(signal.aborted).toBe(true)
  finish({ id: 'job', status: 'done', result: { id } }); await tick()
  expect(document.body.textContent).not.toContain('Скачать видео MKV')
})
it('requires authentication and refuses invalid frame counts before submission', async () => {
  await open(null)
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await unmount(component!); component = undefined; document.body.innerHTML = ''
  await open()
  const input = document.querySelector<HTMLInputElement>('[aria-label="Количество кадров видеоэкспорта Filmcraft"]')!
  for (const value of ['0', '-1', '1.5', '360001', '9007199254740993']) { input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick(); expect(button('Экспортировать видео без звука').disabled).toBe(true) }
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})

it('submits the explicit audio mode with the complete current source bindings', async () => {
  await open()
  const input = document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!
  input.click(); await tick()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ withAudio: true, frames: 48, sources: { '9007199254740993': 'source' } }), expect.any(AbortSignal)))
})

it('reloads persisted history and opens a completed job without submitting another export', async () => {
  const row = { jobId: 'historic', createdAt: 1700000000, job: { id: 'historic', status: 'done' as const, result: { id } } }
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [row], nextCursor: null })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue(row.job)
  await open()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт historic"]')).not.toBeNull())
  await unmount(component!); component = undefined
  await open()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт historic"]')).not.toBeNull())
  document.querySelector<HTMLButtonElement>('[aria-label="Открыть экспорт historic"]')!.click()
  await vi.waitFor(() => expect(button('Скачать видео MKV')).toBeDefined())
  expect(getFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'historic', expect.any(AbortSignal))
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})
it('paginates through an empty filtered page without losing previously loaded entries', async () => {
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValueOnce({ jobs: [{ jobId: 'first', createdAt: 2, job: { id: 'first', status: 'done' } }], nextCursor: 'page-2' }).mockResolvedValueOnce({ jobs: [], nextCursor: 'page-3' }).mockResolvedValue({ jobs: [{ jobId: 'last', createdAt: 1, job: { id: 'last', status: 'cancelled' } }], nextCursor: null })
  await open()
  await vi.waitFor(() => expect(button('Ещё экспорты')).toBeDefined())
  button('Ещё экспорты').click()
  await vi.waitFor(() => expect(listFilmcraftVideoJobs).toHaveBeenCalledWith('owner', 'page-2', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Ещё экспорты').disabled).toBe(false))
  button('Ещё экспорты').click()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт last"]')).not.toBeNull())
  expect(document.querySelector('[aria-label="Открыть экспорт first"]')).not.toBeNull()
  expect(listFilmcraftVideoJobs).toHaveBeenCalledWith('owner', 'page-3', expect.any(AbortSignal))
  expect(button('Ещё экспорты')).toBeUndefined()
})
it('resumes monitoring and cancellation of an active job from history', async () => {
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [{ jobId: 'running-old', createdAt: 2, job: { id: 'running-old', status: 'running', progress: 60 } }], nextCursor: null })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValueOnce({ id: 'running-old', status: 'running', progress: 65 }).mockResolvedValue({ id: 'running-old', status: 'cancelled' })
  await open()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт running-old"]')).not.toBeNull())
  document.querySelector<HTMLButtonElement>('[aria-label="Открыть экспорт running-old"]')!.click()
  await vi.waitFor(() => expect(document.querySelector('progress')?.value).toBe(65))
  button('Отменить видеоэкспорт').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Видеоэкспорт отменён'))
  expect(cancelFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'running-old', expect.any(AbortSignal))
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})
it('aborts history loading on disposal and ignores its late result', async () => {
  let finish!: (value: Awaited<ReturnType<typeof listFilmcraftVideoJobs>>) => void
  vi.mocked(listFilmcraftVideoJobs).mockReturnValue(new Promise(resolve => { finish = resolve }))
  await open()
  expect(listFilmcraftVideoJobs).toHaveBeenCalledWith('owner', undefined, expect.any(AbortSignal))
  const signal = vi.mocked(listFilmcraftVideoJobs).mock.calls[0][2]!
  await unmount(component!); component = undefined
  expect(signal.aborted).toBe(true)
  finish({ jobs: [{ jobId: 'late', createdAt: 1, job: { id: 'late', status: 'done' } }], nextCursor: null }); await tick()
  expect(document.body.textContent).not.toContain('late')
})
it('retries history failures independently of export submission', async () => {
  vi.mocked(listFilmcraftVideoJobs).mockRejectedValueOnce(new Error('История недоступна')).mockResolvedValue({ jobs: [{ jobId: 'restored', createdAt: 1, job: { id: 'restored', status: 'done' } }], nextCursor: null })
  await open()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('История недоступна'))
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
  button('Обновить историю экспорта').click()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт restored"]')).not.toBeNull())
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})
it('clears a previous download when a historical job loses source permission', async () => {
  const done = { jobId: 'allowed', createdAt: 2, job: { id: 'allowed', status: 'done' as const, result: { id } } }
  const denied = { jobId: 'revoked', createdAt: 1, job: { id: 'revoked', status: 'done' as const, result: { id } } }
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [done, denied], nextCursor: null })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValueOnce(done.job).mockRejectedValue(Object.assign(new Error('Нет доступа'), { status: 403 }))
  await open()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт allowed"]')).not.toBeNull())
  document.querySelector<HTMLButtonElement>('[aria-label="Открыть экспорт allowed"]')!.click()
  await vi.waitFor(() => expect(button('Скачать видео MKV')).toBeDefined())
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [done], nextCursor: null })
  document.querySelector<HTMLButtonElement>('[aria-label="Открыть экспорт revoked"]')!.click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Нет доступа'))
  expect(button('Скачать видео MKV')).toBeUndefined()
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
})

it('retries an eligible historical failure using its persisted job id rather than current editor settings', async () => {
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [{ jobId: 'old-failed', createdAt: 1, job: { id: 'old-failed', status: 'error', errorKind: 'storage' } }], nextCursor: null })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValueOnce({ id: 'old-failed', status: 'error', errorKind: 'storage', error: 'Временный сбой' }).mockResolvedValue({ id: 'old-failed', status: 'done', result: { id } })
  vi.mocked(retryFilmcraftVideoJob).mockResolvedValue({ jobId: 'old-failed', status: 'pending' })
  await open()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Открыть экспорт old-failed"]')).not.toBeNull())
  document.querySelector<HTMLButtonElement>('[aria-label="Открыть экспорт old-failed"]')!.click()
  await vi.waitFor(() => expect(button('Повторить видеоэкспорт')).toBeDefined())
  button('Повторить видеоэкспорт').click()
  await vi.waitFor(() => expect(button('Скачать видео MKV')).toBeDefined())
  expect(retryFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'old-failed', expect.any(AbortSignal))
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})
it('surfaces retry-policy refusal and preserves the failed selection', async () => {
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'job', status: 'error', errorKind: 'timeout', error: 'Таймаут' })
  vi.mocked(retryFilmcraftVideoJob).mockRejectedValue(new Error('Лимит попыток исчерпан'))
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(button('Повторить видеоэкспорт')).toBeDefined())
  button('Повторить видеоэкспорт').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Лимит попыток исчерпан'))
  expect(button('Повторить видеоэкспорт').disabled).toBe(false)
})
it('does not offer retry for input validation or permission failures', async () => {
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'job', status: 'error', errorKind: 'validation', error: 'Неверный источник' })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Неверный источник'))
  expect(button('Повторить видеоэкспорт')).toBeUndefined()
})

it('queues a second range while the first poll is unresolved and ignores its later result', async () => {
  let first!: (value: Awaited<ReturnType<typeof getFilmcraftVideoJob>>) => void
  vi.mocked(startFilmcraftVideoJob).mockResolvedValueOnce({ jobId: 'first', mode: 'video_only', container: 'mkv' }).mockResolvedValue({ jobId: 'second', mode: 'video_only', container: 'mkv' })
  vi.mocked(getFilmcraftVideoJob).mockImplementation(async (_token, jobId) => jobId === 'first' ? new Promise(resolve => { first = resolve }) : { id: 'second', status: 'running', progress: 20 })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(getFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'first', expect.any(AbortSignal)))
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
  const start = document.querySelector<HTMLInputElement>('[aria-label="Начальный кадр видеоэкспорта Filmcraft"]')!
  const count = document.querySelector<HTMLInputElement>('[aria-label="Количество кадров видеоэкспорта Filmcraft"]')!
  start.value = '2'; start.dispatchEvent(new Event('input', { bubbles: true })); count.value = '7'; count.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('progress')?.value).toBe(20))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0][1]).toMatchObject({ startFrames: '0', frames: 48 })
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).toMatchObject({ startFrames: '2', frames: 7 })
  first({ id: 'first', status: 'done', result: { id } }); await tick()
  expect(document.querySelector('progress')?.value).toBe(20)
  expect(document.body.textContent).toContain('Задание: second')
  expect(button('Скачать видео MKV')).toBeUndefined()
  expect(cancelFilmcraftVideoJob).not.toHaveBeenCalled()
})
it('keeps the admission lock when an older poll completes during a newer submission', async () => {
  let finishPoll!: (value: Awaited<ReturnType<typeof getFilmcraftVideoJob>>) => void
  let accept!: (value: Awaited<ReturnType<typeof startFilmcraftVideoJob>>) => void
  vi.mocked(startFilmcraftVideoJob).mockResolvedValueOnce({ jobId: 'first', mode: 'video_only', container: 'mkv' }).mockReturnValue(new Promise(resolve => { accept = resolve }))
  vi.mocked(getFilmcraftVideoJob).mockImplementation(async (_token, jobId) => jobId === 'first' ? new Promise(resolve => { finishPoll = resolve }) : { id: 'second', status: 'pending' })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(getFilmcraftVideoJob).toHaveBeenCalled())
  button('Экспортировать видео без звука').click(); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  finishPoll({ id: 'first', status: 'running', progress: 10 }); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  accept({ jobId: 'second', mode: 'video_only', container: 'mkv' })
  await vi.waitFor(() => expect(button('Экспортировать видео без звука').disabled).toBe(false))
  expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2)
})
it('preserves the running monitor and its cancellation when adding another export fails', async () => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValueOnce({ jobId: 'first', mode: 'video_only', container: 'mkv' }).mockRejectedValue(new Error('Очередь переполнена'))
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'first', status: 'running', progress: 10 })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('progress')?.value).toBe(10))
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Очередь переполнена'))
  await vi.waitFor(() => expect(getFilmcraftVideoJob).toHaveBeenCalledTimes(2), { timeout: 2000 })
  expect(document.querySelector('[role=alert]')?.textContent).toContain('Очередь переполнена')
  expect(document.querySelector('progress')?.value).toBe(10)
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'first', status: 'cancelled' })
  button('Отменить видеоэкспорт').click()
  await vi.waitFor(() => expect(cancelFilmcraftVideoJob).toHaveBeenCalledWith('owner', 'first', expect.any(AbortSignal)))
})

it('submits an adaptive H264 preset and downloads the artifact format rather than the edited form format', async () => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'mp4-job', mode: 'video_audio', container: 'mp4' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'mp4-job', status: 'done', result: { id, container: 'mp4' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['mp4']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mp4')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.mp4`) })
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  for (const option of preset.options) option.selected = option.value === 'adaptive_high'; preset.dispatchEvent(new Event('change', { bubbles: true }))
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(button('Скачать видео MP4')).toBeDefined())
  expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'adaptive_high', withAudio: true }), expect.any(AbortSignal))
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать видео MP4').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'mp4')
})

it('submits custom geometry and rejects invalid size or pixel ratios before admission', async () => {
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Свой размер кадра Filmcraft"]')!.click(); await tick()
  const edit = async (label: string, value: string) => { const input = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  await edit('Ширина экспорта Filmcraft', '80')
  await edit('Высота экспорта Filmcraft', '80')
  const scaling = document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!
  scaling.value = 'stretchToFill'; scaling.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой аспект пикселя Filmcraft"]')!.click(); await tick()
  await edit('Числитель аспекта пикселя Filmcraft', '2')
  await edit('Знаменатель аспекта пикселя Filmcraft', '0')
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await edit('Знаменатель аспекта пикселя Filmcraft', '1')
  await edit('Ширина экспорта Filmcraft', '3841')
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await edit('Ширина экспорта Filmcraft', '80')
  document.querySelector<HTMLInputElement>('[aria-label="Высокое качество масштабирования Filmcraft"]')!.click(); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ frameSize: [80, 80], scaling: 'stretchToFill', pixelAspect: [2, 1], maxRenderQuality: true }), expect.any(AbortSignal)))
})

it('submits all metadata fields literally and bounds Unicode metadata bytes', async () => {
  await open()
  const edit = async (label: string, value: string) => { const input = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[aria-label="${label} файла Filmcraft"]`)!; input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  await edit('Название', '🎬'.repeat(4097))
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  expect(document.querySelector('[role="alert"]')?.textContent).toContain('Сократите текст')
  await edit('Название', '  Кадр 🎬 = тест  ')
  await edit('Автор', '  Автор O\'Neil "X"  ')
  await edit('Авторские права', '  © 2026 Video Kadr  ')
  await edit('Описание', '  Первая строка\nВторая строка  ')
  await edit('Комментарий', '  literal $(touch marker); -metadata title=changed  ')
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ metadata: { title: '  Кадр 🎬 = тест  ', creator: '  Автор O\'Neil "X"  ', copyright: '  © 2026 Video Kadr  ', description: '  Первая строка\nВторая строка  ', comment: '  literal $(touch marker); -metadata title=changed  ' } }), expect.any(AbortSignal)))
})

it('submits video limiter thresholds and refuses empty or out-of-range limits', async () => {
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Ограничитель яркости экспорта Filmcraft"]')!.click(); await tick()
  const edit = async (label: string, value: string) => { const input = document.querySelector<HTMLInputElement>(`[aria-label="${label} порог яркости Filmcraft"]`)!; input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  await edit('Нижний', '')
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await edit('Нижний', '10')
  await edit('Верхний', '101')
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await edit('Верхний', '80.5')
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ videoLimiter: { enabled: true, minPercent: 10, maxPercent: 80.5 } }), expect.any(AbortSignal)))
})

it('submits fixed two-pass presets and lets vertical scaling be explicitly overridden', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  const scaling = document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!
  preset.value = 'vimeo1080'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(scaling.disabled).toBe(false)
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'vimeo1080', scaling: 'scaleToFit' }), expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Экспортировать видео без звука').disabled).toBe(false))
  preset.value = 'social_vertical'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(scaling.value).toBe('scaleToFill')
  scaling.value = 'scaleToFit'; scaling.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ preset: 'social_vertical', scaling: 'scaleToFit' }), expect.any(AbortSignal)))
})

it('submits ProRes PCM24 and downloads the completed MOV despite later preset changes', async () => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'mov-job', mode: 'video_audio', container: 'mov' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'mov-job', status: 'done', result: { id, container: 'mov' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['mov']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mov')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.mov`) })
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'prores_hq'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(button('Скачать видео MOV')).toBeDefined())
  expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'prores_hq', withAudio: true }), expect.any(AbortSignal))
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать видео MOV').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'mov')
})

it('submits source-size native DNxHR with PCM24 and preserves explicit pixel fractions', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'dnxhr_hq'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!.disabled).toBe(true)
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит')
  document.querySelector<HTMLInputElement>('[aria-label="Свой аспект пикселя Filmcraft"]')!.click(); await tick()
  for (const [label, value] of [['Числитель', '4294967295'], ['Знаменатель', '4294967294']]) {
    const input = document.querySelector<HTMLInputElement>(`[aria-label="${label} аспекта пикселя Filmcraft"]`)!
    input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  }
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'dnxhr_hq', withAudio: true, pixelAspect: [4294967295, 4294967294] }), expect.any(AbortSignal)))
})

it.each(['apv42210', 'apv42212'])('submits native %s with PCM24 and full pixel fractions', async (selected) => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!.disabled).toBe(true)
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит')
  document.querySelector<HTMLInputElement>('[aria-label="Свой аспект пикселя Filmcraft"]')!.click(); await tick()
  for (const [label, value] of [['Числитель', '4294967295'], ['Знаменатель', '4294967294']]) {
    const input = document.querySelector<HTMLInputElement>(`[aria-label="${label} аспекта пикселя Filmcraft"]`)!
    input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  }
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: selected, withAudio: true, pixelAspect: [4294967295, 4294967294] }), expect.any(AbortSignal)))
})

it.each(['mxf_dnxhr_hq', 'mxf_prores_hq', 'mxf_h264'])('submits %s and downloads its completed MXF despite preset changes', async (selected) => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'mxf-job', mode: 'video_audio', container: 'mxf' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'mxf-job', status: 'done', result: { id, container: 'mxf' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['mxf']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mxf')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.mxf`) })
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(button('Скачать видео MXF')).toBeDefined())
  expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: selected, withAudio: true }), expect.any(AbortSignal))
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать видео MXF').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'mxf')
})

it.each(['mxf_atom_dnxhr_hq'])('submits %s and downloads its completed OP-Atom bundle despite preset changes', async (selected) => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'mxf-job', mode: 'video_audio', container: 'zip' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'mxf-job', status: 'done', result: { id, container: 'zip' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['zip']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:mxf')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.zip`) })
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(button('Скачать набор ZIP')).toBeDefined())
  expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: selected, withAudio: true }), expect.any(AbortSignal))
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать набор ZIP').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'zip')
})

it.each(['png_sequence', 'tiff_sequence', 'bmp_sequence'])('submits %s without stale audio and downloads the completed sequence', async (selected) => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'images', mode: 'video_only', container: 'zip' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'images', status: 'done', result: { id, container: 'zip' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['zip']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:images')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.zip`) })
  await open()
  const audio = document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!
  audio.click(); await tick(); expect(audio.checked).toBe(true)
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(audio.checked).toBe(false); expect(audio.disabled).toBe(true)
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!.disabled).toBe(true)
  button('Экспортировать последовательность').click()
  await vi.waitFor(() => expect(button('Скачать набор ZIP')).toBeDefined())
  const input = vi.mocked(startFilmcraftVideoJob).mock.calls.at(-1)![1]
  expect(input.preset).toBe(selected); expect(input.withAudio).toBeUndefined()
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать набор ZIP').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'zip')
})

it.each(['gif'])('submits %s without stale audio and downloads the completed sequence', async (selected) => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'images', mode: 'video_only', container: 'gif' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'images', status: 'done', result: { id, container: 'gif' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['zip']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:images')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.gif`) })
  await open()
  const audio = document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!
  audio.click(); await tick(); expect(audio.checked).toBe(true)
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(audio.checked).toBe(false); expect(audio.disabled).toBe(true)
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Масштабирование экспорта Filmcraft"]')!.disabled).toBe(false)
  button('Экспортировать GIF').click()
  await vi.waitFor(() => expect(button('Скачать видео GIF')).toBeDefined())
  const input = vi.mocked(startFilmcraftVideoJob).mock.calls.at(-1)![1]
  expect(input.preset).toBe(selected); expect(input.withAudio).toBeUndefined()
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Скачать видео GIF').click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), 'gif')
})

it.each([['wav16', 'wav'], ['wav24', 'wav'], ['aiff16', 'aiff']])('submits %s as audio only and keeps completed format on download', async (selected, format) => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'audio', mode: 'audio_only', container: format as 'wav' | 'aiff' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'audio', status: 'done', result: { id, container: format } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['audio']))
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:audio')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.${format}`) })
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Свой размер кадра Filmcraft"]')!.click(); await tick()
  const width = document.querySelector<HTMLInputElement>('[aria-label="Ширина экспорта Filmcraft"]')!
  width.value = 'invalid'; width.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой аспект пикселя Filmcraft"]')!.click(); await tick()
  const pixel = document.querySelector<HTMLInputElement>('[aria-label="Числитель аспекта пикселя Filmcraft"]')!
  pixel.value = 'invalid'; pixel.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('[aria-label="Свой размер кадра Filmcraft"]')).toBeNull()
  expect(document.querySelector('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')).toBeNull()
  expect(document.querySelector('section')?.textContent).toContain('48 кГц · стерео')
  button('Экспортировать аудио').click()
  await vi.waitFor(() => expect(button(`Скачать аудио ${format.toUpperCase()}`)).toBeDefined())
  const input = vi.mocked(startFilmcraftVideoJob).mock.calls.at(-1)![1]
  expect(input).toEqual(expect.objectContaining({ preset: selected, withAudio: true }))
  expect(input.frameSize).toBeUndefined(); expect(input.pixelAspect).toBeUndefined(); expect(input.scaling).toBeUndefined(); expect(input.videoLimiter).toBeUndefined()
  preset.value = 'lossless'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button(`Скачать аудио ${format.toUpperCase()}`).click()
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner', id, expect.any(AbortSignal), format)
})

it('submits audio loudness settings and displays the completed measured report', async () => {
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'loudness', mode: 'audio_only', container: 'wav' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'loudness', status: 'done', result: { id, container: 'wav', loudness: { measuredLufs: -31.25, gainDb: 8.25 } } })
  await open()
  expect(document.querySelector('[aria-label="Нормализация громкости Filmcraft"]')).toBeNull()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'wav24'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Нормализация громкости Filmcraft"]')!.click(); await tick()
  const peak = document.querySelector<HTMLInputElement>('[aria-label="Предел true peak Filmcraft"]')!
  peak.value = '1'; peak.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать аудио').disabled).toBe(true)
  for (const [label, value] of [['Целевая громкость LUFS Filmcraft', '-23'], ['Предел true peak Filmcraft', '-3']]) {
    const input = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!
    input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  }
  button('Экспортировать аудио').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ loudness: { enabled: true, targetLufs: -23, truePeakDbtp: -3 } }), expect.any(AbortSignal)))
  await vi.waitFor(() => expect(document.querySelector('section')?.textContent).toContain('измерено -31.25 LUFS'))
  expect(document.querySelector('section')?.textContent).toContain('усиление 8.25 дБ')
})

it('ignores hidden stale loudness values for a silent image export', async () => {
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Нормализация громкости Filmcraft"]')!.click(); await tick()
  const target = document.querySelector<HTMLInputElement>('[aria-label="Целевая громкость LUFS Filmcraft"]')!
  target.value = ''; target.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'png_sequence'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('[aria-label="Нормализация громкости Filmcraft"]')).toBeNull()
  button('Экспортировать последовательность').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalled())
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls.at(-1)![1].loudness).toBeUndefined()
})

it('submits independent name and sequence timecode overlay settings', async () => {
  await open()
  const input = (label: string) => document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!
  input('Подпись экспорта Filmcraft').click()
  input('Таймкод экспорта Filmcraft').click()
  await tick()
  const text = document.querySelector<HTMLTextAreaElement>('[aria-label="Подпись: текст Filmcraft"]')!
  text.value = 'Кадр 🎬'; text.dispatchEvent(new Event('input', { bubbles: true }))
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Таймкод: положение Filmcraft"]')!
  select.selectedIndex = 2; select.dispatchEvent(new Event('change', { bubbles: true }))
  const opacity = input('Подпись: непрозрачность Filmcraft')
  opacity.value = '50'; opacity.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({
    nameOverlay: { enabled: true, text: 'Кадр 🎬', placement: 'bottomCenter', offset: [0, 0], sizePercent: 5, opacity: 50 },
    timecodeOverlay: { enabled: true, text: '', placement: 'topRight', offset: [0, 0], sizePercent: 5, opacity: 100 }
  }), expect.any(AbortSignal)))
})

it('rejects an empty enabled overlay size and omits disabled overlay settings', async () => {
  await open()
  const enabled = document.querySelector<HTMLInputElement>('[aria-label="Подпись экспорта Filmcraft"]')!
  enabled.click(); await tick()
  const size = document.querySelector<HTMLInputElement>('[aria-label="Подпись: размер Filmcraft"]')!
  size.value = ''; size.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  enabled.click(); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalled())
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0][1]).not.toHaveProperty('nameOverlay')
})

it('requires a library image and submits independent watermark geometry', async () => {
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Изображение экспорта Filmcraft"]')!.click(); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Файл изображения экспорта Filmcraft"]')!
  select.value = 'logo'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const opacity = document.querySelector<HTMLInputElement>('[aria-label="Изображение: непрозрачность Filmcraft"]')!
  opacity.value = '42'; opacity.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ imageOverlay: { enabled: true, sourceId: 'logo', placement: 'topRight', offset: [0, 0], sizePercent: 15, opacity: 42 } }), expect.any(AbortSignal)))
})

it('validates a rational output rate and preserves source range identifiers', async () => {
  await open()
  document.querySelector<HTMLInputElement>('[aria-label="Своя частота кадров Filmcraft"]')!.click(); await tick()
  const edit = async (label: string, value: string) => { const input = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  await edit('Знаменатель частоты кадров Filmcraft', '0')
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  await edit('Числитель частоты кадров Filmcraft', '30000')
  await edit('Знаменатель частоты кадров Filmcraft', '1001')
  await edit('Начальный кадр видеоэкспорта Filmcraft', '9007199254740993')
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ startFrames: '9007199254740993', frames: 48, frameRate: { numerator: 30000, denominator: 1001 } }), expect.any(AbortSignal)))
})

it('validates and submits a custom H264 keyframe distance and omits it for other codecs', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'mxf_h264'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой интервал ключевых кадров Filmcraft"]')!.click(); await tick()
  const distance = document.querySelector<HTMLInputElement>('[aria-label="Интервал ключевых кадров Filmcraft"]')!
  distance.value = '0'; distance.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  distance.value = '3'; distance.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'mxf_h264', keyframeDistance: 3 }), expect.any(AbortSignal)))
  preset.value = 'prores_hq'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('[aria-label="Интервал ключевых кадров Filmcraft"]')).toBeNull()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('keyframeDistance')
})

it('submits the chosen H264 profile and hides it when switching to another codec', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'mxf_h264'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const profile = document.querySelector<HTMLSelectElement>('[aria-label="Профиль H264 Filmcraft"]')!
  profile.value = 'baseline'; profile.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'mxf_h264', h264Profile: 'baseline' }), expect.any(AbortSignal)))
  preset.value = 'prores_hq'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('[aria-label="Профиль H264 Filmcraft"]')).toBeNull()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('h264Profile')
})

it('submits adaptive two-pass bitrate settings and omits them for another codec', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'mxf_h264'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой битрейт H264 Filmcraft"]')!.click(); await tick()
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим битрейта H264 Filmcraft"]')!
  mode.value = 'vbr2Pass'; mode.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Адаптивный битрейт H264 Filmcraft"]')!.click()
  document.querySelector<HTMLInputElement>('[aria-label="Свой максимум битрейта H264 Filmcraft"]')!.click(); await tick()
  const bpp = document.querySelector<HTMLInputElement>('[aria-label="Бит на пиксель H264 Filmcraft"]')!
  bpp.value = ''; bpp.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  bpp.value = '0.25'; bpp.dispatchEvent(new Event('input', { bubbles: true }))
  const max = document.querySelector<HTMLInputElement>('[aria-label="Максимальный битрейт H264 Filmcraft"]')!
  max.value = '500'; max.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ bitrate: { mode: 'vbr2Pass', targetKbps: 10000, maxKbps: 500, adaptiveBpp: 0.25 } }), expect.any(AbortSignal)))
  preset.value = 'prores_hq'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('bitrate')
})

it('allows native MP4 CBR with and without audio', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'adaptive_high'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой битрейт H264 Filmcraft"]')!.click(); await tick()
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим битрейта H264 Filmcraft"]')!
  mode.value = 'cbr'; mode.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(false)
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(button('Экспортировать видео со звуком').disabled).toBe(false)
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ withAudio: true, bitrate: { mode: 'cbr', targetKbps: 10000 } }), expect.any(AbortSignal)))
})

it('submits H264 MOV with PCM24 and omits movie-only settings for MXF', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  await select('Контейнер H264 Filmcraft', 'mov')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Аудиокодек H264 Filmcraft', 'pcm')
  await select('Разрядность PCM H264 Filmcraft', '24')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ multiplexer: 'mov', h264Audio: { codec: 'pcm', bits: 24, bitrateKbps: 320, sampleRate: 48000 } }), expect.any(AbortSignal)))
  await select('Пресет видеоэкспорта Filmcraft', 'mxf_h264')
  expect(document.querySelector('[aria-label="Контейнер H264 Filmcraft"]')).toBeNull()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('multiplexer')
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('h264Audio')
})

it('validates and submits a custom AAC bitrate', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'adaptive_high'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  const bitrate = document.querySelector<HTMLInputElement>('[aria-label="Битрейт AAC Filmcraft"]')!
  bitrate.value = '513'; bitrate.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
  bitrate.value = '128'; bitrate.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ h264Audio: { codec: 'auto', bits: 16, bitrateKbps: 128, sampleRate: 48000 } }), expect.any(AbortSignal)))
})

it('submits audio rate, displays the low-rate AAC cap and omits inactive rates', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Частота аудио H264 Filmcraft', '8000')
  expect(document.body.textContent).toContain('AAC 96 кбит/с · 8 кГц')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ h264Audio: { codec: 'auto', bits: 16, bitrateKbps: 320, sampleRate: 8000 } }), expect.any(AbortSignal)))
  await select('Пресет видеоэкспорта Filmcraft', 'prores_hq')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('h264Audio')
})

it('restores an AAC-compatible rate when switching from high-rate PCM', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  await select('Контейнер H264 Filmcraft', 'mov')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Аудиокодек H264 Filmcraft', 'pcm')
  await select('Частота аудио H264 Filmcraft', '192000')
  expect(button('Экспортировать видео со звуком').disabled).toBe(false)
  await select('Аудиокодек H264 Filmcraft', 'aac')
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Частота аудио H264 Filmcraft"]')!.value).toBe('48000')
  expect(button('Экспортировать видео со звуком').disabled).toBe(false)
  await select('Частота аудио H264 Filmcraft', '44100')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ h264Audio: { codec: 'aac', bits: 16, bitrateKbps: 320, sampleRate: 44100 } }), expect.any(AbortSignal)))
})


it('uses the sequence audio rate in Auto and preserves explicit 48 kHz overrides', async () => {
  const sequenceId = '18446744073709551615'
  await open('owner', JSON.stringify({ project: { items: { [sequenceId]: { kind: { Sequence: { settings: { sample_rate: 44100 } } } } } } }))
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Частота аудио H264 Filmcraft"]')!.value).toBe('48000')
  expect(document.body.textContent).toContain('Авто · секвенция 44.1 кГц')
  expect(document.body.textContent).toContain('AAC 320 кбит/с · 48 кГц')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(1))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0][1]).not.toHaveProperty('h264Audio')
  await select('Частота аудио H264 Filmcraft', '0')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).toHaveProperty('h264Audio.sampleRate', 0)
  await select('Частота аудио H264 Filmcraft', '48000')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(3))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[2][1]).not.toHaveProperty('h264Audio')
})

it('admits the authored high rate for PCM and requires a supported rate for AAC', async () => {
  const sequenceId = '18446744073709551615'
  await open('owner', JSON.stringify({ project: { items: { [sequenceId]: { kind: { Sequence: { settings: { sample_rate: 192000 } } } } } } }))
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Частота аудио H264 Filmcraft', '0')
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
  await select('Контейнер H264 Filmcraft', 'mov')
  // Switching to MP4 is the operation that resets an incompatible auto rate;
  // MOV still allows choosing the authored rate for PCM explicitly.
  await select('Аудиокодек H264 Filmcraft', 'pcm')
  expect(button('Экспортировать видео со звуком').disabled).toBe(false)
  expect(document.body.textContent).toContain('PCM 16 бит · 192 кГц')
  await select('Аудиокодек H264 Filmcraft', 'aac')
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Частота аудио H264 Filmcraft"]')!.value).toBe('48000')
  expect(button('Экспортировать видео со звуком').disabled).toBe(false)
})

it('submits mono audio, displays its AAC bitrate cap and omits channels outside H264 movies', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Каналы аудио H264 Filmcraft', '1')
  await select('Частота аудио H264 Filmcraft', '8000')
  expect(document.body.textContent).toContain('AAC 48 кбит/с · 8 кГц · моно')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ h264Audio: { codec: 'auto', bits: 16, bitrateKbps: 320, sampleRate: 8000, channels: 1 } }), expect.any(AbortSignal)))
  await select('Контейнер H264 Filmcraft', 'mov')
  await select('Аудиокодек H264 Filmcraft', 'pcm')
  expect(document.body.textContent).toContain('PCM 16 бит · 8 кГц · моно')
  await select('Пресет видеоэкспорта Filmcraft', 'prores_hq')
  expect(document.querySelector('[aria-label="Каналы аудио H264 Filmcraft"]')).toBeNull()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).not.toHaveProperty('h264Audio')
})

it('submits 5.1 movie audio and shows its effective AAC bitrate', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await select('Каналы аудио H264 Filmcraft', '6')
  await select('Частота аудио H264 Filmcraft', '8000')
  expect(document.body.textContent).toContain('AAC 288 кбит/с · 8 кГц · 5.1')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ h264Audio: { codec: 'auto', bits: 16, bitrateKbps: 320, sampleRate: 8000, channels: 6 } }), expect.any(AbortSignal)))
  await select('Контейнер H264 Filmcraft', 'mov')
  await select('Аудиокодек H264 Filmcraft', 'pcm')
  expect(document.body.textContent).toContain('PCM 16 бит · 8 кГц · 5.1')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(2))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[1][1]).toMatchObject({ multiplexer: 'mov', h264Audio: { codec: 'pcm', bits: 16, sampleRate: 8000, channels: 6 } })
})

it('submits PCM rate, surround channels and 24-bit AIFF and preserves common audio settings', async () => {
  await open()
  const select = async (label: string, value: string) => { const input = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
  await select('Пресет видеоэкспорта Filmcraft', 'aiff16')
  await select('Каналы PCM Filmcraft', '6')
  await select('Частота PCM Filmcraft', '192000')
  await select('Разрядность PCM Filmcraft', '24')
  expect(document.body.textContent).toContain('PCM 24 бит · 192 кГц · 5.1')
  button('Экспортировать аудио').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'aiff16', audio: { channels: 6, bits: 24, sampleRate: 192000 } }), expect.any(AbortSignal)))
  await select('Пресет видеоэкспорта Filmcraft', 'adaptive_high')
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Каналы аудио H264 Filmcraft"]')!.value).toBe('6')
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Частота аудио H264 Filmcraft"]')!.value).toBe('192000')
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
})

it('keeps WAV24 preset bits when no PCM override is authored', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'wav24'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Разрядность PCM Filmcraft"]')!.value).toBe('24')
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Частота PCM Filmcraft"]')!.value).toBe('48000')
  button('Экспортировать аудио').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(1))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0][1]).not.toHaveProperty('audio')
})

it.each(['prores_hq', 'dnxhr_hq', 'apv42210'])('submits configurable PCM and AAC for native %s MOV', async (selected) => {
  await open()
  const change = async (label: string, value: string) => {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value
    control.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
  }
  await change('Пресет видеоэкспорта Filmcraft', selected)
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click()
  await tick()
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Разрядность PCM MOV Filmcraft"]')!.value).toBe('24')
  await change('Каналы аудио MOV Filmcraft', '6')
  await change('Частота аудио MOV Filmcraft', '192000')
  await change('Разрядность PCM MOV Filmcraft', '16')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ preset: selected, audio: { codec: 'auto', channels: 6, bits: 16, sampleRate: 192000, bitrateKbps: 320 } }), expect.any(AbortSignal)))
  await change('Аудиокодек MOV Filmcraft', 'aac')
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
  await change('Частота аудио MOV Filmcraft', '8000')
  const bitrate = document.querySelector<HTMLInputElement>('[aria-label="Битрейт AAC Filmcraft"]')!
  bitrate.value = '512'; bitrate.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ preset: selected, audio: { codec: 'aac', channels: 6, bits: 16, sampleRate: 8000, bitrateKbps: 512 } }), expect.any(AbortSignal)))
})

it('resolves native MOV Auto rate from the sequence while retaining the 48k preset default', async () => {
  await open('owner', JSON.stringify({ project: { items: { '18446744073709551615': { kind: { Sequence: { settings: { sample_rate: 44117 } } } } } } }))
  const change = async (label: string, value: string) => {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value; control.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  }
  await change('Пресет видеоэкспорта Filmcraft', 'prores_hq')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит · 48 кГц')
  await change('Частота аудио MOV Filmcraft', '0')
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит · 44.117 кГц')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ audio: expect.objectContaining({ sampleRate: 0 }) }), expect.any(AbortSignal)))
  await change('Аудиокодек MOV Filmcraft', 'aac')
  expect(button('Экспортировать видео со звуком').disabled).toBe(true)
})

it.each(['mxf_dnxhr_hq', 'mxf_prores_hq', 'mxf_h264', 'mxf_atom_dnxhr_hq'])('submits configurable PCM for %s and hides unsupported AAC', async (selected) => {
  await open()
  const change = async (label: string, value: string) => {
    const control = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!
    control.value = value; control.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  }
  await change('Пресет видеоэкспорта Filmcraft', 'prores_hq')
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  await change('Аудиокодек MOV Filmcraft', 'aac')
  await change('Пресет видеоэкспорта Filmcraft', selected)
  expect(document.querySelector('[aria-label="Аудиокодек MOV Filmcraft"]')).toBeNull()
  expect(document.querySelector('[aria-label="Битрейт AAC Filmcraft"]')).toBeNull()
  await change('Каналы аудио MXF Filmcraft', '6')
  await change('Частота аудио MXF Filmcraft', '192000')
  await change('Разрядность PCM MXF Filmcraft', '16')
  expect(document.querySelector('section')?.textContent).toContain('PCM 16 бит · 192 кГц · 5.1')
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ preset: selected, audio: { codec: 'pcm', channels: 6, bits: 16, sampleRate: 192000, bitrateKbps: 320 } }), expect.any(AbortSignal)))
})

it.each(['adaptive_high', 'adaptive_medium', 'adaptive_low', 'high_quality1080', 'high_quality2160', 'youtube1080', 'youtube2160', 'vimeo1080', 'vimeo2160', 'social_vertical', 'wav16', 'wav24', 'aiff16'])('uses the source 48k audio preset for %s on a non-AAC authored rate', async (selected) => {
  await open('owner', JSON.stringify({ project: { items: { '18446744073709551615': { kind: { Sequence: { settings: { sample_rate: 44117 } } } } } } }))
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = selected; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const audioOnly = ['wav16', 'wav24', 'aiff16'].includes(selected)
  if (!audioOnly) { document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick() }
  const rate = document.querySelector<HTMLSelectElement>(`[aria-label="${audioOnly ? 'Частота PCM Filmcraft' : 'Частота аудио H264 Filmcraft'}"]`)!
  expect(rate.value).toBe('48000')
  const submit = button(audioOnly ? 'Экспортировать аудио' : 'Экспортировать видео со звуком')
  expect(submit.disabled).toBe(false); submit.click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledTimes(1))
  const request = vi.mocked(startFilmcraftVideoJob).mock.calls[0][1]
  expect(request).not.toHaveProperty('audio'); expect(request).not.toHaveProperty('h264Audio')
})
it('submits explicit WAV Auto without losing the named preset default', async () => {
  await open('owner', JSON.stringify({ project: { items: { '18446744073709551615': { kind: { Sequence: { settings: { sample_rate: 44117 } } } } } } }))
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'wav24'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const rate = document.querySelector<HTMLSelectElement>('[aria-label="Частота PCM Filmcraft"]')!
  rate.value = '0'; rate.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('section')?.textContent).toContain('PCM 24 бит · 44.117 кГц')
  button('Экспортировать аудио').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ audio: { channels: 2, bits: 24, sampleRate: 0 } }), expect.any(AbortSignal)))
})

it('submits a saved preset by name without stale manual export overrides', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [{ name: 'Review 720p', category: 'User', description: 'client', settings: {}, builtin: false, favorite: false }], default: 'Review 720p' })
  await open()
  button('Библиотека пресетов').click()
  await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined())
  button('Применить пресет').click(); await tick()
  expect(document.querySelector('[aria-label="Пресет видеоэкспорта Filmcraft"]')).toBeNull()
  button('Экспортировать с сохранённым пресетом').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', { document: '{"original":true}', sequenceId: '18446744073709551615', startFrames: '0', frames: 48, sources: { '9007199254740993': 'source' }, libraryPreset: 'Review 720p' }, expect.any(AbortSignal)))
})

it('downloads binary caption bytes from the authorized completed job result', async () => {
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'job', status: 'done', result: { id, filename: `${id}.mkv`, captionSidecar: { extension: 'stl', bytesBase64: btoa(String.fromCharCode(0, 255, 1, 128)) } } })
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:captions')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe(`${id}.stl`) })
  await open(); button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(button('Скачать субтитры STL')).toBeDefined())
  button('Скачать субтитры STL').click()
  expect(click).toHaveBeenCalledOnce()
  const blob=create.mock.calls[0][0] as Blob;expect(blob.size).toBe(4);expect(blob.type).toBe('application/octet-stream')
  expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([0, 255, 1, 128])
  await unmount(component!); component=undefined;expect(revoke).toHaveBeenCalledWith('blob:captions')
})

it('quick export delegates its range to the backend instead of stale manual fields', async () => {
  vi.mocked(startFilmcraftQuickExportJob).mockResolvedValue({ jobId: 'quick-job', mode: 'video_audio', container: 'mp4', presetName: 'Match Source – Adaptive High Bitrate' })
  await open()
  const frames=document.querySelector<HTMLInputElement>('[aria-label="Количество кадров видеоэкспорта Filmcraft"]')!;frames.value='invalid';frames.dispatchEvent(new Event('input',{bubbles:true}));await tick()
  expect(button('Быстрый экспорт').disabled).toBe(false)
  button('Быстрый экспорт').click()
  await vi.waitFor(() => expect(startFilmcraftQuickExportJob).toHaveBeenCalledWith('owner', { document: '{"original":true}', sequenceId: '18446744073709551615', sources: { '9007199254740993': 'source' } }, expect.any(AbortSignal)))
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
  await vi.waitFor(() => expect(getFilmcraftVideoJob).toHaveBeenCalledWith('owner','quick-job',expect.any(AbortSignal)))
  expect(document.body.textContent).toContain('Последний быстрый экспорт: Match Source – Adaptive High Bitrate')
})

it('quick export uses the explicitly selected library preset', async () => {
  vi.mocked(listFilmcraftExportPresets).mockResolvedValue({ presets: [{ name: 'Review', category: 'User', description: '', settings: {}, builtin: false, favorite: false }], default: 'Review' })
  vi.mocked(startFilmcraftQuickExportJob).mockResolvedValue({ jobId: 'quick-job', mode: 'video_only', container: 'mp4', presetName: 'Review' })
  await open();button('Библиотека пресетов').click();await vi.waitFor(() => expect(button('Применить пресет')).toBeDefined());button('Применить пресет').click();await tick()
  button('Быстрый экспорт').click()
  await vi.waitFor(() => expect(startFilmcraftQuickExportJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'Review' }), expect.any(AbortSignal)))
})

it('submits native Motion JPEG quality and MOV audio, refusing invalid quality first', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'mjpeg'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const quality = document.querySelector<HTMLInputElement>('[aria-label="Качество Motion JPEG"]')!
  for (const value of ['', '0', '101', '1.5']) {
    quality.value = value; quality.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    expect(button('Экспортировать видео без звука').disabled).toBe(true)
  }
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
  quality.value = '65'; quality.dispatchEvent(new Event('input', { bubbles: true }))
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  button('Экспортировать видео со звуком').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'mjpeg', mjpegQuality: 65, withAudio: true, audio: expect.objectContaining({ codec: 'auto', bits: 24, channels: 2, sampleRate: 48000 }) }), expect.any(AbortSignal)))
})
it('omits hidden Motion JPEG quality when another export codec is selected', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'mjpeg'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const quality = document.querySelector<HTMLInputElement>('[aria-label="Качество Motion JPEG"]')!
  quality.value = ''; quality.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  preset.value = 'adaptive_high'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalled())
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0]![1]).not.toHaveProperty('mjpegQuality')
})
it('submits native HEVC bitrate and MOV settings without H264 profile', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'hevc'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector('[aria-label="Профиль H264 Filmcraft"]')).toBeNull()
  const container = document.querySelector<HTMLSelectElement>('[aria-label="Контейнер HEVC Filmcraft"]')!
  container.value = 'mov'; container.dispatchEvent(new Event('change', { bubbles: true }))
  document.querySelector<HTMLInputElement>('[aria-label="Свой битрейт HEVC Filmcraft"]')!.click(); await tick()
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим битрейта HEVC Filmcraft"]')!
  expect(mode.querySelector<HTMLOptionElement>('[value="vbr2Pass"]')!.disabled).toBe(true)
  mode.value = 'cbr'; mode.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Экспортировать видео без звука').click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ preset: 'hevc', multiplexer: 'mov', bitrate: expect.objectContaining({ mode: 'cbr', targetKbps: 10000 }) }), expect.any(AbortSignal)))
  expect(vi.mocked(startFilmcraftVideoJob).mock.calls[0]![1]).not.toHaveProperty('h264Profile')
})
it('blocks stale two-pass settings when changing H264 to HEVC', async () => {
  await open()
  const preset = document.querySelector<HTMLSelectElement>('[aria-label="Пресет видеоэкспорта Filmcraft"]')!
  preset.value = 'adaptive_high'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Свой битрейт H264 Filmcraft"]')!.click(); await tick()
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим битрейта H264 Filmcraft"]')!
  mode.value = 'vbr2Pass'; mode.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  preset.value = 'hevc'; preset.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(button('Экспортировать видео без звука').disabled).toBe(true)
  expect(startFilmcraftVideoJob).not.toHaveBeenCalled()
})
