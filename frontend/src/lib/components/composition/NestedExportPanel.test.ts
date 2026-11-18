// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cancelJob, downloadFilmcraftVideo, exportNestedVideo, getJob, listNestedExports, nestedDurations, nestedExportCursorTimecode, retryFilmcraftVideoJob, setNestedExportPriority, moveNestedExport, writeCaptionFile, type NativeNestedProject } from '$lib/api'
import { loadLibrary } from '$lib/state/store.svelte'
import { closeNestedComposition, nestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedExportPanel from './NestedExportPanel.svelte'
vi.mock('$lib/api', () => ({ cancelJob: vi.fn(), downloadFilmcraftVideo: vi.fn(), exportNestedVideo: vi.fn(), getJob: vi.fn(), listNestedExports: vi.fn(), nestedDurations: vi.fn(), nestedExportCursorTimecode: vi.fn(), retryFilmcraftVideoJob: vi.fn(), setNestedExportPriority: vi.fn(), moveNestedExport: vi.fn(), writeCaptionFile: vi.fn() }))
vi.mock('$lib/state/store.svelte', () => ({ loadLibrary: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
  closeNestedComposition()
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content: {tracks:[]}, instances: [] } } } as unknown as NativeNestedProject
  nestedComposition.renderedOutputs = { root: 'output' }
  vi.mocked(nestedDurations).mockResolvedValue({ root: 1000000 })
  vi.mocked(exportNestedVideo).mockResolvedValue({ jobId: 'job-1', mode: 'video_only', container: 'mkv' })
  vi.mocked(getJob).mockResolvedValue({ id: 'job-1', status: 'done', result: { filename: 'result.mkv' } })
})
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); vi.resetAllMocks(); document.body.innerHTML = '' })
function button(label: string): HTMLButtonElement { return [...document.querySelectorAll('button')].find(b => b.textContent?.trim() === label)! }
async function ready() { component = mount(NestedExportPanel, { target: document.body, props: { token: 'owner-token' } }); await vi.waitFor(() => expect(button('Экспортировать видео').disabled).toBe(false)); await tick() }
it('exports the native focus/range/bindings and refreshes the library after completion', async () => {
  await ready()
  const end = document.querySelector<HTMLInputElement>('input[aria-label="Конец нативного экспорта"]')!
  end.value = '750000'; end.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(exportNestedVideo).toHaveBeenCalledWith(nestedComposition.document, 'root', 0, 750000, { root: 'output' }, 'owner-token', expect.any(AbortSignal), 'video_only')
  await tick(); expect(document.body.textContent).toContain('Готово в медиатеке: result.mkv')
})
it('submits a per-job name and uses the returned name for the video and caption downloads', async () => {
  const id = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(getJob).mockResolvedValue({ id: 'job-1', status: 'done', result: {
    id, filename: `${id}.mkv`, downloadName: 'Монтаж 2.mkv',
    captionSidecar: { extension: 'vtt', bytesBase64: btoa('WEBVTT') },
  } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['video']))
  const downloads: string[] = []
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
    downloads.push(this.download)
  })
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:export')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  try {
    await ready()
    const field = document.querySelector<HTMLInputElement>('input[placeholder="Автоматически"]')!
    field.value = '  Монтаж 2  '
    field.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    button('Экспортировать видео').click()
    await vi.waitFor(() => expect(document.body.textContent).toContain('Готово в медиатеке: Монтаж 2.mkv'))
    expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[11]).toBe('Монтаж 2')
    button('Скачать готовое видео').click()
    await vi.waitFor(() => expect(downloads).toEqual(['Монтаж 2.mkv']))
    button('Скачать субтитры VTT').click()
    expect(downloads).toEqual(['Монтаж 2.mkv', 'Монтаж 2.vtt'])
  } finally {
    click.mockRestore(); create.mockRestore(); revoke.mockRestore()
  }
})

it('exports and downloads a standalone WAV using the selected audio-only mode', async () => {
  const id = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(exportNestedVideo).mockResolvedValue({ jobId: 'job-1', mode: 'audio_only', container: 'wav' })
  vi.mocked(getJob).mockResolvedValue({ id: 'job-1', status: 'done', result: { id, filename: `${id}.wav`, container: 'wav' } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['wav']))
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:wav')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  try {
    await ready()
    const label = [...document.querySelectorAll('label')].find(item => item.textContent?.trim() === 'Только звук')!
    label.querySelector('input')!.click()
    await tick()
    button('Экспортировать звук').click()
    await vi.waitFor(() => expect(button('Скачать готовый звук')).toBeDefined())
    expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[7]).toBe('audio_only')
    button('Скачать готовый звук').click()
    await vi.waitFor(() => expect(click).toHaveBeenCalledOnce())
    expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner-token', id, expect.any(AbortSignal), 'wav')
  } finally {
    click.mockRestore(); create.mockRestore(); revoke.mockRestore()
  }
})

it('submits the chosen compressed audio codec and validates bitrate before queuing', async () => {
  await ready()
  const input = (label: string) => [...document.querySelectorAll('label')]
    .find(item => item.textContent?.trim().startsWith(label))!.querySelector('input')!
  input('Только звук').click()
  await tick()
  const codec = document.querySelector<HTMLSelectElement>('[aria-label="Формат звука"]')!
  codec.value = 'opus'
  codec.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  const channels = document.querySelector<HTMLSelectElement>('[aria-label="Каналы звука"]')!
  channels.value = '1'
  channels.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  const bitrate = input('Битрейт')
  expect(bitrate.max).toBe('256')
  bitrate.value = '320'
  bitrate.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  expect(button('Экспортировать звук').disabled).toBe(true)
  bitrate.value = '160'
  bitrate.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  const call = vi.mocked(exportNestedVideo).mock.calls[0]
  expect(call?.[12]).toEqual({ codec: 'opus', bitrateKbps: 160 })
  expect(call?.[13]).toBe(1)
})

it('submits the chosen AAC sample rate and applies the codec bitrate ceiling', async () => {
  await ready()
  const label = [...document.querySelectorAll('label')].find(item => item.textContent?.trim() === 'Только звук')!
  label.querySelector('input')!.click()
  await tick()
  for (const [field, value] of [['Формат звука', 'aac'], ['Каналы звука', '1'], ['Частота дискретизации', '44100']]) {
    const select = document.querySelector<HTMLSelectElement>(`[aria-label="${field}"]`)!
    select.value = value!
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
  }
  const bitrate = [...document.querySelectorAll('label')].find(item => item.textContent?.trim().startsWith('Битрейт'))!.querySelector('input')!
  expect(bitrate.max).toBe('264')
  button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[14]).toBe(44100)
})

it('sets exact export bounds from the playhead and restores the full timeline', async () => {
  await ready()
  const start = document.querySelector<HTMLInputElement>('[aria-label="Начало нативного экспорта"]')!
  const end = document.querySelector<HTMLInputElement>('[aria-label="Конец нативного экспорта"]')!
  nestedComposition.timeUs = 250001
  await tick()
  button('Начало по курсору').click()
  nestedComposition.timeUs = 750001
  await tick()
  button('Конец по курсору').click()
  await tick()
  expect([start.value, end.value]).toEqual(['250001', '750001'])
  expect(button('Начало по курсору').disabled).toBe(true)

  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(vi.mocked(exportNestedVideo).mock.calls[0]?.slice(1, 4)).toEqual(['root', 250001, 750001])
  button('Весь таймлайн').click()
  await tick()
  expect([start.value, end.value]).toEqual(['0', '1000000'])
  expect(button('Весь таймлайн').disabled).toBe(true)
})

it('submits the entered SMPTE range through the native export request', async () => {
  const project = nestedComposition.document!
  const root = project.compositions.root!
  project.compositions.root = {
    ...root,
    content: {
      ...root.content,
      canvas: { ...root.content.canvas, fpsMilli: 29970, fpsRate: { numerator: 30000, denominator: 1001 } },
      captionTracks: [{
        id: 'file', name: 'Caption file', clips: [{ id: 'cue', timelineStartTick: 0, timelineEndTick: 500000, text: 'Caption' }],
      }],
    },
  }
  vi.mocked(exportNestedVideo).mockResolvedValue({
    jobId: 'job-1', mode: 'video_only', container: 'mkv',
    startUs: 60026633, endUs: 60060000,
    captionRange: { startTick: '15247725292800', endTick: '15256200960000' },
  })
  vi.mocked(writeCaptionFile).mockResolvedValue({ extension: 'srt', dataBase64: '' })
  await ready()
  const mode = [...document.querySelectorAll('label')].find(label => label.textContent?.includes('Диапазон по таймкоду'))!
  mode.querySelector('input')!.click()
  await tick()
  expect(button('Экспортировать видео').disabled).toBe(true)
  for (const [label, value] of [['Начальный таймкод', '00:00:59;29'], ['Конечный таймкод', '00:01:00;02']]) {
    const field = [...document.querySelectorAll('label')].find(node => node.textContent?.trim() === label)!
      .querySelector('input')!
    field.value = value!
    field.dispatchEvent(new Event('input', { bubbles: true }))
  }
  await tick()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[10]).toEqual({ start: '00:00:59;29', end: '00:01:00;02' })
  const captions = document.querySelector('[aria-label="Субтитры к готовому видео"]')!
  captions.querySelector<HTMLButtonElement>('button')!.click()
  await vi.waitFor(() => expect(writeCaptionFile).toHaveBeenCalledWith(expect.anything(), 'srt', {
    dropFrame: true, rate: { num: 30000, den: 1001 }, offsetTick: '15247725292800', endTick: '15256200960000',
  }))
})

it('fills both SMPTE fields from the cursor using the native clock and drop-frame setting', async () => {
  const project = nestedComposition.document!
  const root = project.compositions.root!
  const rate = { numerator: 30000, denominator: 1001 }
  project.compositions.root = { ...root, content: {
    ...root.content, canvas: { ...root.content.canvas, fpsMilli: 29970, fpsRate: rate },
    metadata: { ...root.content.metadata, dropFrame: true },
  } }
  vi.mocked(nestedDurations).mockResolvedValue({ root: 65000000 })
  vi.mocked(nestedExportCursorTimecode).mockResolvedValueOnce({ timecode: '00:00:59;29' })
    .mockResolvedValueOnce({ timecode: '00:01:00;02' })
  await ready()
  const mode = [...document.querySelectorAll('label')].find(label => label.textContent?.includes('Диапазон по таймкоду'))!
  mode.querySelector('input')!.click()
  await tick()
  nestedComposition.timeUs = 60026633
  await tick()
  button('Начало по курсору').click()
  await vi.waitFor(() => expect(button('Конец по курсору').disabled).toBe(false))
  nestedComposition.timeUs = 60060000
  await tick()
  button('Конец по курсору').click()
  await vi.waitFor(() => expect(button('Экспортировать видео').disabled).toBe(false))
  expect(nestedExportCursorTimecode).toHaveBeenNthCalledWith(1, 60026633, rate, true, expect.any(AbortSignal))
  expect(nestedExportCursorTimecode).toHaveBeenNthCalledWith(2, 60060000, rate, true, expect.any(AbortSignal))
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[10]).toEqual({ start: '00:00:59;29', end: '00:01:00;02' })
})

it('retains the accepted job after a network failure and resumes observation without resubmitting', async () => {
  vi.mocked(getJob).mockRejectedValueOnce(new Error('network unavailable'))
  await ready(); button('Экспортировать видео').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('network unavailable')); await tick()
  expect(button('Экспортировать видео').disabled).toBe(true)
  button('Проверить задачу').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(exportNestedVideo).toHaveBeenCalledOnce(); expect(getJob).toHaveBeenCalledTimes(2)
})
it('queues another range while preserving observation of the first export', async () => {
  let finish!: (value: Awaited<ReturnType<typeof getJob>>) => void
  vi.mocked(getJob).mockReturnValueOnce(new Promise(resolve => { finish = resolve }))
  vi.mocked(exportNestedVideo)
    .mockResolvedValueOnce({ jobId: 'job-1', mode: 'video_only', container: 'mkv' })
    .mockRejectedValueOnce(new Error('queue unavailable'))
    .mockResolvedValueOnce({ jobId: 'job-2', mode: 'video_only', container: 'mkv' })
  await ready()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(button('Добавить экспорт в очередь')?.disabled).toBe(false))

  const end = document.querySelector<HTMLInputElement>('input[aria-label="Конец нативного экспорта"]')!
  end.value = '750000'
  end.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  button('Добавить экспорт в очередь').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('queue unavailable'))
  button('Добавить экспорт в очередь').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Добавлена задача экспорта: job-2'))

  expect(vi.mocked(exportNestedVideo).mock.calls.map(call => call.slice(1, 4)))
    .toEqual([['root', 0, 1000000], ['root', 0, 750000], ['root', 0, 750000]])
  expect(document.querySelector('.job-id')?.textContent).toContain('job-1')
  expect(document.querySelector('progress')).not.toBeNull()
  expect(getJob).toHaveBeenCalledOnce()
  expect(document.body.textContent).not.toContain('queue unavailable')

  finish({ id: 'job-1', status: 'done', result: { filename: 'first.mkv' } })
  await vi.waitFor(() => expect(document.body.textContent).toContain('Готово в медиатеке: first.mkv'))
})

it('switches from a running export through history and discards the previous response', async () => {
  let release!: (value: Awaited<ReturnType<typeof getJob>>) => void
  vi.mocked(getJob)
    .mockReturnValueOnce(new Promise(resolve => { release = resolve }))
    .mockResolvedValueOnce({ id: 'job-2', status: 'done', result: { filename: 'second.mkv' } })
    .mockResolvedValueOnce({ id: 'job-1', status: 'running', progress: 40 })
  vi.mocked(listNestedExports).mockResolvedValue({
    jobs: [{ jobId: 'job-2', status: 'done', createdAt: 42 },
           { jobId: 'job-1', status: 'running', createdAt: 41 }],
    nextCursor: null,
  })
  await ready()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(getJob).toHaveBeenCalledOnce())
  const priorSignal = vi.mocked(getJob).mock.calls[0]![1]
  button('Обновить список задач').click()
  await vi.waitFor(() => expect(document.querySelector('li button[aria-current="true"]')?.textContent).toContain('В работе'))
  const rows = document.querySelectorAll<HTMLButtonElement>('li button')
  expect(rows[0]?.disabled).toBe(false)
  rows[0]!.click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Готово в медиатеке: second.mkv'))
  expect(priorSignal?.aborted).toBe(true)
  expect(cancelJob).not.toHaveBeenCalled()

  release({ id: 'job-1', status: 'done', result: { filename: 'stale.mkv' } })
  await tick()
  expect(document.body.textContent).not.toContain('stale.mkv')
  expect(document.querySelector('.job-id')?.textContent).toContain('job-2')
  rows[1]!.click()
  await vi.waitFor(() => expect(document.querySelector('progress')?.value).toBe(40))
  expect(document.querySelector('.job-id')?.textContent).toContain('job-1')
  expect(cancelJob).not.toHaveBeenCalled()
})

it('requests cancellation and aborts observation on disposal without cancelling the server job again', async () => {
  let resolve!: (value: Awaited<ReturnType<typeof getJob>>) => void
  vi.mocked(getJob).mockReturnValue(new Promise(done => { resolve = done }))
  await ready(); button('Экспортировать видео').click()
  await vi.waitFor(() => expect(getJob).toHaveBeenCalledOnce()); await tick()
  button('Отменить экспорт').click(); await vi.waitFor(() => expect(cancelJob).toHaveBeenCalledWith('job-1', 'owner-token'))
  const signal = vi.mocked(getJob).mock.calls[0]![1]!
  await unmount(component!); component = undefined
  expect(signal.aborted).toBe(true)
  resolve({ id: 'job-1', status: 'done' }); await tick()
  expect(loadLibrary).not.toHaveBeenCalled(); expect(cancelJob).toHaveBeenCalledOnce()
})

it('submits the explicitly selected native audio mode', async () => {
  await ready()
  const checkbox = document.querySelector<HTMLInputElement>('input[type="checkbox"]')!
  checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.body.textContent).toContain('звук PCM')
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(vi.mocked(exportNestedVideo).mock.calls[0]![7]).toBe('video_audio')
})

it('submits the selected integer PCM depth for FFV1 video', async () => {
  await ready()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в нативный экспорт"]')!.click()
  await tick()
  const depth = document.querySelector<HTMLSelectElement>('[aria-label="Разрядность PCM"]')!
  depth.value = 's24'
  depth.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  const call = vi.mocked(exportNestedVideo).mock.calls[0]!
  expect(call[7]).toBe('video_audio')
  expect(call[12]).toBeUndefined()
  expect(call[15]).toBe('s24')
})

it('submits mono AAC at 44.1 kHz for FFV1 video', async () => {
  await ready()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в нативный экспорт"]')!.click()
  await tick()
  const codec = document.querySelector<HTMLSelectElement>('[aria-label="Кодек звука"]')!
  codec.value = 'aac'
  codec.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(document.body.textContent).toContain('FFV1 / MKV · звук AAC')
  for (const [field, value] of [['Каналы звука', '1'], ['Частота дискретизации', '44100']]) {
    const select = document.querySelector<HTMLSelectElement>(`[aria-label="${field}"]`)!
    select.value = value
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
  }
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  const call = vi.mocked(exportNestedVideo).mock.calls[0]!
  expect(call[7]).toBe('video_audio')
  expect(call[12]).toEqual({ codec: 'aac', bitrateKbps: 192 })
  expect(call[13]).toBe(1)
  expect(call[14]).toBe(44100)
})

it('exports bound video masks with the saved project revision',async()=>{
 nestedComposition.projectId='saved';nestedComposition.revision=9
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',clips:[{enabled:true,concatCutout:{subject:'person',sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}}]}]},instances:[]}}} as unknown as NativeNestedProject
 await ready();button('Экспортировать видео').click();await vi.waitFor(()=>expect(loadLibrary).toHaveBeenCalledOnce());expect(vi.mocked(exportNestedVideo).mock.calls[0]![8]).toEqual({projectId:'saved',expectedRevision:9})
})

it('exports a chosen subtitle track using both boundaries of the video range', async () => {
  const node = nestedComposition.document!.compositions.root!
  node.content = { ...node.content, canvas: { ...node.content.canvas, fpsMilli: 24000 }, captionTracks: [
    { id: 'one', name: 'First', clips: [] },
    { id: 'two', name: 'Dialogue', vttBlocks: ['NOTE keep'], clips: [
      { id: 'cue', text: 'Dialogue', timelineStartTick: '1', timelineEndTick: '254016000000' },
    ] },
  ] }
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:matched-caption')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const outputId = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(getJob).mockResolvedValue({ id: 'job-1', status: 'done', result: { id: outputId, filename: `${outputId}.mkv` } })
  vi.mocked(downloadFilmcraftVideo).mockResolvedValue(new Blob(['video']))
  const linkClick = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  vi.mocked(writeCaptionFile).mockResolvedValue({ extension: 'srt', dataBase64: btoa('subtitle') })
  const change = (field: HTMLInputElement | HTMLSelectElement, value: string, event: string) => {
    field.value = value
    field.dispatchEvent(new Event(event, { bubbles: true }))
  }
  try {
    await ready()
    const start = document.querySelector<HTMLInputElement>('[aria-label="Начало нативного экспорта"]')!
    const end = document.querySelector<HTMLInputElement>('[aria-label="Конец нативного экспорта"]')!
    const track = document.querySelector<HTMLSelectElement>('[aria-label="Дорожка для диапазона видео"]')!
    change(start, '250001', 'input')
    change(end, '750001', 'input')
    change(track, 'two', 'change')
    await tick()
    button('Экспортировать дорожку субтитров').click()
    await vi.waitFor(() => expect(create).toHaveBeenCalledOnce())
    expect(writeCaptionFile).toHaveBeenCalledWith(expect.objectContaining({ blocks: ['NOTE keep'] }), 'srt', {
      dropFrame: true, rate: { num: 24000, den: 1000 }, offsetTick: '63504254016', endTick: '190512254016',
    })
    button('Экспортировать видео').click()
    await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
    expect(vi.mocked(exportNestedVideo).mock.calls[0]?.slice(1, 4)).toEqual(['root', 250001, 750001])
    await vi.waitFor(() => expect(button('Экспортировать видео').disabled).toBe(false))
    change(end, '750002', 'input')
    await tick()
    expect(document.querySelector('a[download]')).toBeNull()
    expect(revoke).toHaveBeenCalledWith('blob:matched-caption')
    const nextNode = nestedComposition.document!.compositions.root!
    nextNode.content = { ...nextNode.content, captionTracks: [] }
    await tick()
    const result = document.querySelector('[aria-label="Субтитры к готовому видео"]')!
    result.querySelector('button')!.click()
    await vi.waitFor(() => expect(result.querySelector('a[download]')).not.toBeNull())
    expect(writeCaptionFile).toHaveBeenLastCalledWith(expect.objectContaining({
      blocks: ['NOTE keep'], cues: [expect.objectContaining({ text: 'Dialogue' })],
    }), 'srt', { dropFrame: true, rate: { num: 24000, den: 1000 }, offsetTick: '63504254016', endTick: '190512254016' })
    expect(result.querySelector<HTMLAnchorElement>('a[download]')?.download).toBe(`${outputId}.srt`)
    button('Скачать готовое видео').click()
    await vi.waitFor(() => expect(linkClick).toHaveBeenCalledOnce())
    expect(downloadFilmcraftVideo).toHaveBeenCalledWith('owner-token', outputId, expect.any(AbortSignal))
  } finally {
    linkClick.mockRestore()
    create.mockRestore()
    revoke.mockRestore()
  }
})

it('omits caption pixels and invalidates dependent renders without changing the authored project', async () => {
  const project = nestedComposition.document!
  const root = project.compositions.root!
  project.compositions.child = {
    ...root,
    content: { ...root.content, captionTracks: [{
      id: 'file', name: 'File', clips: [{ id: 'cue', text: 'Caption', timelineStartTick: 0, timelineEndTick: 500000 }],
    }], tracks: [
      { id: 'captions', kind: 'text', name: 'Imported text', hidden: false, clips: [{ id: 'text-cue' }] },
      { id: 'ordinary', kind: 'text', name: 'Title', hidden: false, clips: [{ id: 'title' }] },
    ], metadata: { captionTextTracks: ['captions'] } } as unknown as typeof root.content,
  }
  project.compositions.other = { ...root }
  root.instances = [{ timing: { composition: 'child' } }] as unknown as typeof root.instances
  nestedComposition.renderedOutputs = { root: 'old-root', child: 'old-child', other: 'keep' }
  const before = JSON.stringify(project)
  await ready()
  const checkbox = [...document.querySelectorAll('label')].find(item =>
    item.textContent?.trim() === 'Вшить субтитры в видео')!.querySelector('input')!
  expect(checkbox.checked).toBe(true)
  checkbox.checked = false
  checkbox.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  button('Экспортировать видео').click()
  await vi.waitFor(() => expect(exportNestedVideo).toHaveBeenCalledOnce())
  const [copy, , , , outputs] = vi.mocked(exportNestedVideo).mock.calls[0]!
  expect(copy.compositions.child!.content.captionTracks?.[0]?.hidden).toBe(true)
  expect(copy.compositions.child!.content.tracks.map(track => 'hidden' in track && track.hidden)).toEqual([true, false])
  expect(outputs).toEqual({ other: 'keep' })
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old-root', child: 'old-child', other: 'keep' })
})

it('reopens a job and downloads the persisted caption artifact without the original panel state', async () => {
  const node = nestedComposition.document!.compositions.root!
  node.content = { ...node.content, canvas: { ...node.content.canvas, fpsMilli: 24000 }, captionTracks: [
    { id: 'file', name: 'Dialogue', clips: [{ id: 'cue', text: 'Dialogue', timelineStartTick: 0, timelineEndTick: 1_000_000 }] },
  ] }
  const id = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(exportNestedVideo).mockResolvedValue({ jobId: id, mode: 'video_only', container: 'mkv' })
  vi.mocked(getJob).mockResolvedValue({
    id, status: 'done', result: { id, filename: `${id}.mkv`, captionSidecar: { extension: 'vtt', bytesBase64: btoa('WEBVTT\n') } },
  })
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:stored-caption')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  try {
    await ready()
    const format = document.querySelector<HTMLSelectElement>('[aria-label="Файл субтитров в задаче"]')!
    format.value = 'vtt'
    format.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    button('Экспортировать видео').click()
    await vi.waitFor(() => expect(button('Скачать субтитры VTT')).toBeDefined())
    expect(vi.mocked(exportNestedVideo).mock.calls[0]?.[9]).toEqual({ trackId: 'file', format: 'vtt', dropFrame: true })
    await unmount(component!)
    component = undefined
    node.content = { ...node.content, captionTracks: [] }
    await ready()
    vi.mocked(listNestedExports)
      .mockResolvedValueOnce({ jobs: [{ jobId: 'latest', status: 'running', createdAt: 1_700_000_001 }], nextCursor: 'latest' })
      .mockResolvedValueOnce({ jobs: [{ jobId: id, status: 'done', createdAt: 1_700_000_000 }], nextCursor: null })
    button('Обновить список задач').click()
    await vi.waitFor(() => expect(button('Показать более ранние задачи')).toBeDefined())
    button('Показать более ранние задачи').click()
    await vi.waitFor(() => expect(document.querySelectorAll('details ul button')).toHaveLength(2))
    document.querySelectorAll<HTMLButtonElement>('details ul button')[1]!.click()
    expect(listNestedExports).toHaveBeenNthCalledWith(1, 'owner-token', expect.any(AbortSignal))
    expect(listNestedExports).toHaveBeenNthCalledWith(2, 'owner-token', expect.any(AbortSignal), 'latest')
    expect(button('Показать более ранние задачи')).toBeUndefined()
    expect(getJob).toHaveBeenLastCalledWith(id, expect.any(AbortSignal), 'owner-token')
    await vi.waitFor(() => expect(button('Скачать субтитры VTT')).toBeDefined())
    button('Скачать субтитры VTT').click()
    expect(click.mock.instances[0]).toMatchObject({ download: `${id}.vtt` })
    expect(await (create.mock.calls[0]?.[0] as Blob).text()).toBe('WEBVTT\n')
    expect(writeCaptionFile).not.toHaveBeenCalled()
    expect(getJob).toHaveBeenCalledTimes(2)
    await vi.waitFor(() => expect(revoke).toHaveBeenCalledWith('blob:stored-caption'))
  } finally {
    click.mockRestore()
    create.mockRestore()
    revoke.mockRestore()
  }
})

it('retries a failed export by its existing job ID without submitting the edited range', async () => {
  const id = '12345678-1234-1234-1234-123456789abc'
  vi.mocked(getJob)
    .mockResolvedValueOnce({ id, status: 'error', errorKind: 'timeout', error: 'Timed out' })
    .mockResolvedValueOnce({ id, status: 'done', result: {
      id, filename: `${id}.mkv`, captionSidecar: { extension: 'vtt', bytesBase64: btoa('WEBVTT\n') },
    } })
  vi.mocked(retryFilmcraftVideoJob).mockRejectedValueOnce(new Error('Retry unavailable'))
    .mockResolvedValueOnce({ jobId: id, status: 'pending' })
  await ready()
  const field = [...document.querySelectorAll('label')].find(label => label.textContent?.trim() === 'Идентификатор задачи')!.querySelector('input')!
  field.value = id
  field.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  button('Открыть задачу').click()
  await vi.waitFor(() => expect(button('Повторить экспорт')).toBeDefined())
  button('Повторить экспорт').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Retry unavailable'))
  expect(button('Повторить экспорт').disabled).toBe(false)
  button('Повторить экспорт').click()
  await vi.waitFor(() => expect(button('Скачать субтитры VTT')).toBeDefined())
  expect(retryFilmcraftVideoJob).toHaveBeenCalledTimes(2)
  expect(retryFilmcraftVideoJob).toHaveBeenLastCalledWith('owner-token', id, expect.any(AbortSignal))
  expect(getJob).toHaveBeenLastCalledWith(id, expect.any(AbortSignal), 'owner-token')
  expect(exportNestedVideo).not.toHaveBeenCalled()
})

it('can raise and reset priority for an export still in the queue', async () => {
  vi.mocked(listNestedExports).mockResolvedValue({
    jobs: [{ jobId: 'waiting', status: 'pending', createdAt: 42, raised: false },
           { jobId: 'running', status: 'running', createdAt: 41 }],
    nextCursor: null,
  })
  vi.mocked(setNestedExportPriority)
    .mockResolvedValueOnce({ jobId: 'waiting', raised: true })
    .mockResolvedValueOnce({ jobId: 'waiting', raised: false })
  await ready()
  button('Обновить список задач').click()
  await vi.waitFor(() => expect(button('Повысить приоритет')).toBeDefined())
  expect([...document.querySelectorAll('button')].filter(b => b.textContent?.trim() === 'Повысить приоритет')).toHaveLength(1)
  button('Повысить приоритет').click()
  await vi.waitFor(() => expect(button('Вернуть обычный приоритет')).toBeDefined())
  button('Вернуть обычный приоритет').click()
  await vi.waitFor(() => expect(button('Повысить приоритет')).toBeDefined())
  expect(setNestedExportPriority).toHaveBeenNthCalledWith(1, 'waiting', true, 'owner-token', expect.any(AbortSignal))
  expect(setNestedExportPriority).toHaveBeenNthCalledWith(2, 'waiting', false, 'owner-token', expect.any(AbortSignal))
})

it('refreshes waiting jobs while history is open and aborts watching when closed', async () => {
  vi.mocked(listNestedExports).mockResolvedValue({
    jobs: [{ jobId: 'waiting', status: 'pending', createdAt: 42, raised: true }],
    nextCursor: null,
  })
  vi.mocked(getJob).mockResolvedValueOnce({ id: 'waiting', status: 'running' })
  await ready()
  const details = [...document.querySelectorAll('details')].find(node =>
    node.querySelector('summary')?.textContent?.trim() === 'Недавние задачи экспорта')!
  details.open = true
  details.dispatchEvent(new Event('toggle'))
  await vi.waitFor(() => expect(button('Вернуть обычный приоритет')).toBeDefined())
  await vi.waitFor(() => expect(Boolean(button('Вернуть обычный приоритет'))).toBe(false), { timeout: 4000 })
  expect(details.textContent).toContain('В работе')
  const signal = vi.mocked(getJob).mock.calls[0]![1]
  expect(signal?.aborted).toBe(false)
  details.open = false
  details.dispatchEvent(new Event('toggle'))
  await vi.waitFor(() => expect(signal?.aborted).toBe(true))
  expect(getJob).toHaveBeenCalledTimes(1)
})

it('moves ready exports using the server order and updates both positions', async () => {
  const queue = [{ jobId: 'first', raised: false }, { jobId: 'second', raised: false }]
  vi.mocked(listNestedExports).mockResolvedValue({
    jobs: [
      { jobId: 'second', status: 'pending', createdAt: 42 },
      { jobId: 'first', status: 'pending', createdAt: 41 },
    ],
    queue, nextCursor: null,
  })
  vi.mocked(moveNestedExport).mockResolvedValue({
    jobId: 'second', applied: true, queue: [queue[1]!, queue[0]!],
  })
  await ready()
  button('Обновить список задач').click()
  await vi.waitFor(() => expect(document.querySelectorAll('details li')).toHaveLength(2))
  const rows = [...document.querySelectorAll('details li')]
  const move = (row: Element, label: string) =>
    [...row.querySelectorAll('button')].find(button => button.textContent?.trim() === label)!
  expect(rows[0]?.textContent).toContain('Место в вашей очереди: 2')
  expect(move(rows[0]!, 'Раньше').disabled).toBe(false)
  expect(move(rows[0]!, 'Позже').disabled).toBe(true)
  move(rows[0]!, 'Раньше').click()
  await vi.waitFor(() => expect(rows[0]?.textContent).toContain('Место в вашей очереди: 1'))
  expect(rows[1]?.textContent).toContain('Место в вашей очереди: 2')
  expect(move(rows[0]!, 'Раньше').disabled).toBe(true)
  expect(moveNestedExport).toHaveBeenCalledWith('second', 'earlier', 'owner-token', expect.any(AbortSignal))
})

it('cancels a queued export directly from history and updates the remaining position', async () => {
  vi.mocked(listNestedExports).mockResolvedValue({
    jobs: [
      { jobId: 'first', status: 'pending', createdAt: 42 },
      { jobId: 'second', status: 'pending', createdAt: 41 },
    ],
    queue: [{ jobId: 'first', raised: false }, { jobId: 'second', raised: false }],
    nextCursor: null,
  })
  vi.mocked(getJob).mockResolvedValueOnce({ id: 'first', status: 'cancelled' })
  await ready()
  button('Обновить список задач').click()
  await vi.waitFor(() => expect(document.querySelectorAll('details li')).toHaveLength(2))
  const rows = [...document.querySelectorAll('details li')]
  expect(rows[1]?.textContent).toContain('Место в вашей очереди: 2')
  button('Отменить задачу').click()
  await vi.waitFor(() => expect(rows[0]?.textContent).toContain('Отменено'))
  expect(rows[0]?.textContent).not.toContain('Отменить задачу')
  expect(rows[1]?.textContent).toContain('Место в вашей очереди: 1')
  expect(cancelJob).toHaveBeenCalledWith('first', 'owner-token')
  expect(getJob).toHaveBeenCalledWith('first', expect.any(AbortSignal), 'owner-token')
})
