// @ts-expect-error Tests use the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { startFilmcraftAnalysis, listFilmcraftAnalysis, cancelFilmcraftAnalysis } from '$lib/api'
import { observeAnalysis } from '$lib/filmcraft/analysis-client'
import type { Job } from '$lib/types'
import Controls from './FilmcraftAnalysisControls.svelte'
import Harness from './test-fixtures/AnalysisControlsHarness.svelte'
vi.mock('$lib/api', () => ({ startFilmcraftAnalysis: vi.fn(), listFilmcraftAnalysis: vi.fn(), cancelFilmcraftAnalysis: vi.fn() }))
vi.mock('$lib/filmcraft/analysis-client', () => ({ observeAnalysis: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const input = { document: 'first', sequenceId: '6', timeFrames: '9007199254740993', sources: { '1': 'video' } }
const ready = { id: 'job', status: 'done' as const, progress: 100, result: { ready: true } }
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.innerHTML = ''; vi.resetAllMocks() })
const button = (name: string) => Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b => b.textContent === name)!
function setup(onready = vi.fn(), token: string | null = 'owner') {
 vi.mocked(listFilmcraftAnalysis).mockResolvedValue({ jobs: [], nextCursor: null }); vi.mocked(startFilmcraftAnalysis).mockResolvedValue({ jobId: 'job' });
 component = mount(Controls, { target: document.body, props: { input, token, onready } }); return onready
}
it('submits exact snapshot, displays progress and refreshes current preview once', async () => {
 const onready = setup(); let finish!: (value: Job) => void
 vi.mocked(observeAnalysis).mockImplementation((_owner, _id, _signal, update) => { update({ id: 'job', status: 'running', progress: 42 }); return new Promise(resolve => { finish = resolve }) })
 await tick(); button('Подготовить эффекты в фоне').click(); await vi.waitFor(() => expect(document.querySelector('[role=status]')?.textContent).toContain('42%'))
 expect(startFilmcraftAnalysis).toHaveBeenCalledWith('owner', input, expect.any(AbortSignal)); expect(document.querySelector('progress')?.value).toBe(42)
 finish(ready); await vi.waitFor(() => expect(onready).toHaveBeenCalledTimes(1)); expect(cancelFilmcraftAnalysis).not.toHaveBeenCalled()
 vi.mocked(observeAnalysis).mockResolvedValue(ready); button('Обновить состояние').click(); await tick(); expect(onready).toHaveBeenCalledTimes(1)
})
it('detaches observation on unmount without cancelling durable work', async () => {
 setup(); vi.mocked(observeAnalysis).mockImplementation(() => new Promise(() => {})); await tick(); button('Подготовить эффекты в фоне').click(); await vi.waitFor(() => expect(observeAnalysis).toHaveBeenCalledTimes(1)); const signal = vi.mocked(observeAnalysis).mock.calls[0]![2]
 await unmount(component!); component = undefined; expect(signal.aborted).toBe(true); expect(cancelFilmcraftAnalysis).not.toHaveBeenCalled()
})
it('cancels explicitly and retains an actionable failure', async () => {
 setup(); vi.mocked(observeAnalysis).mockImplementation((_a,_b,_c,update) => { update({ id: 'job', status: 'running' }); return new Promise(() => {}) }); vi.mocked(cancelFilmcraftAnalysis).mockRejectedValue(new Error('offline'))
 await tick(); button('Подготовить эффекты в фоне').click(); await vi.waitFor(() => expect(button('Отменить анализ')).toBeDefined()); button('Отменить анализ').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Не удалось отменить: offline')); expect(cancelFilmcraftAnalysis).toHaveBeenCalledWith('owner','job',expect.any(AbortSignal)); expect(button('Отменить анализ').disabled).toBe(false)
})
it('reconnects after observation failure without creating another job', async () => {
 const onready = setup(); vi.mocked(observeAnalysis).mockRejectedValueOnce(new Error('offline')).mockResolvedValue(ready)
 await tick(); button('Подготовить эффекты в фоне').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('offline')); button('Обновить состояние').click(); await vi.waitFor(() => expect(onready).toHaveBeenCalledTimes(1)); expect(startFilmcraftAnalysis).toHaveBeenCalledTimes(1)
})
it('loads owner history, paginates and reconnects without refreshing unrelated project', async () => {
 const onready = setup(); vi.mocked(listFilmcraftAnalysis).mockResolvedValueOnce({ jobs: [{ jobId: 'old', status: 'running', progress: 20, createdAt: 1 }], nextCursor: 'old' }).mockResolvedValue({ jobs: [{ jobId: 'older', status: 'done', progress: 100, createdAt: 0 }], nextCursor: null }); vi.mocked(observeAnalysis).mockResolvedValue(ready)
 await vi.waitFor(() => expect(button('Ещё задания')).toBeDefined()); button('Ещё задания').click(); await vi.waitFor(() => expect(listFilmcraftAnalysis).toHaveBeenCalledWith('owner','old',expect.any(AbortSignal)))
 const old = Array.from(document.querySelectorAll<HTMLButtonElement>('li button')).find(b=>b.textContent?.includes('Анализируется'))!; old.click(); await vi.waitFor(() => expect(observeAnalysis).toHaveBeenCalledWith('owner','old',expect.any(AbortSignal),expect.any(Function))); expect(onready).not.toHaveBeenCalled(); expect(startFilmcraftAnalysis).not.toHaveBeenCalled()
})
it('suppresses late submit/result on project and owner changes', async () => {
 const onready = vi.fn(); vi.mocked(listFilmcraftAnalysis).mockResolvedValue({ jobs: [], nextCursor: null }); let submitted!: (value: {jobId:string}) => void; vi.mocked(startFilmcraftAnalysis).mockImplementation(()=>new Promise(resolve=>{submitted=resolve}));
 component=mount(Harness,{target:document.body,props:{onready}});await tick();button('Подготовить эффекты в фоне').click();await vi.waitFor(()=>expect(startFilmcraftAnalysis).toHaveBeenCalledTimes(1));const submitSignal=vi.mocked(startFilmcraftAnalysis).mock.calls[0]![2]!;button('Сменить проект').click();await tick();expect(submitSignal.aborted).toBe(true);submitted({jobId:'stale'});await tick();expect(observeAnalysis).not.toHaveBeenCalled()
 vi.mocked(startFilmcraftAnalysis).mockResolvedValue({jobId:'new'});let finish!:(job:Job)=>void;vi.mocked(observeAnalysis).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));button('Подготовить эффекты в фоне').click();await vi.waitFor(()=>expect(observeAnalysis).toHaveBeenCalledTimes(1));const signal=vi.mocked(observeAnalysis).mock.calls[0]![2];button('Сменить пользователя').click();await tick();expect(signal.aborted).toBe(true);finish(ready);await tick();expect(onready).not.toHaveBeenCalled()
})
it('requires a signed-in owner', async () => { setup(vi.fn(),null);await tick();expect(button('Подготовить эффекты в фоне').disabled).toBe(true);expect(listFilmcraftAnalysis).not.toHaveBeenCalled() })
