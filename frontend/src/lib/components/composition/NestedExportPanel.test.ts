// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cancelJob, downloadFilmcraftVideo, exportNestedVideo, getJob, listNestedExports, nestedDurations, retryFilmcraftVideoJob, writeCaptionFile, type NativeNestedProject } from '$lib/api'
import { loadLibrary } from '$lib/state/store.svelte'
import { closeNestedComposition, nestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedExportPanel from './NestedExportPanel.svelte'
vi.mock('$lib/api', () => ({ cancelJob: vi.fn(), downloadFilmcraftVideo: vi.fn(), exportNestedVideo: vi.fn(), getJob: vi.fn(), listNestedExports: vi.fn(), nestedDurations: vi.fn(), retryFilmcraftVideoJob: vi.fn(), writeCaptionFile: vi.fn() }))
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
it('retains the accepted job after a network failure and resumes observation without resubmitting', async () => {
  vi.mocked(getJob).mockRejectedValueOnce(new Error('network unavailable'))
  await ready(); button('Экспортировать видео').click()
  await vi.waitFor(() => expect(document.body.textContent).toContain('network unavailable')); await tick()
  expect(button('Экспортировать видео').disabled).toBe(true)
  button('Проверить задачу').click()
  await vi.waitFor(() => expect(loadLibrary).toHaveBeenCalledOnce())
  expect(exportNestedVideo).toHaveBeenCalledOnce(); expect(getJob).toHaveBeenCalledTimes(2)
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
