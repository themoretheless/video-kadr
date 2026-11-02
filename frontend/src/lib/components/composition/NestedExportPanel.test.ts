// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cancelJob, exportNestedVideo, getJob, nestedDurations, type NativeNestedProject } from '$lib/api'
import { loadLibrary } from '$lib/state/store.svelte'
import { closeNestedComposition, nestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedExportPanel from './NestedExportPanel.svelte'
vi.mock('$lib/api', () => ({ cancelJob: vi.fn(), exportNestedVideo: vi.fn(), getJob: vi.fn(), nestedDurations: vi.fn() }))
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
  button('Отменить экспорт').click(); await vi.waitFor(() => expect(cancelJob).toHaveBeenCalledWith('job-1'))
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
