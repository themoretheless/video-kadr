// @ts-expect-error Tests use the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { listSamModels, startSamModelDownload, listSamModelDownloads, cancelSpeechModelDownload, retrySpeechModelDownload } from '$lib/api'
import { observeModelDownload } from '$lib/speech-model-download-client'
import type { Job } from '$lib/types'
import Controls from './SamModelDownloads.svelte'
import Harness from './test-fixtures/SamDownloadsHarness.svelte'
vi.mock('$lib/api', () => ({listSamModels:vi.fn(),startSamModelDownload:vi.fn(),listSamModelDownloads:vi.fn(),cancelSpeechModelDownload:vi.fn(),retrySpeechModelDownload:vi.fn()}))
vi.mock('$lib/speech-model-download-client',()=>({observeModelDownload:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const button=(name:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent===name)!
const done:Job={id:'job',status:'done',progress:100,result:{id:'hash'}}
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.resetAllMocks()})
async function setup(){
 vi.mocked(listSamModelDownloads).mockResolvedValue({jobs:[],nextCursor:null})
 vi.mocked(listSamModels).mockResolvedValue({selectedId:'configured',models:[],catalog:[{id:'slimsam-77-uniform',name:'SlimSAM',bytes:39833906,revision:'pinned'}]})
 vi.mocked(startSamModelDownload).mockResolvedValue({jobId:'job'})
 const onready=vi.fn();component=mount(Controls,{target:document.body,props:{token:'owner',onready}});await tick();button('Загрузить каталог моделей').click();await vi.waitFor(()=>expect(button('Скачать модель').disabled).toBe(false));return onready
}
it('downloads selected catalog entry, displays progress and refreshes model library once',async()=>{
 const ready=await setup();let finish!:(job:Job)=>void
 vi.mocked(observeModelDownload).mockImplementation((_a,_b,_c,update)=>{update({id:'job',status:'running',progress:42});return new Promise(resolve=>{finish=resolve})})
 button('Скачать модель').click();await vi.waitFor(()=>expect(document.querySelector('progress')?.value).toBe(42));expect(startSamModelDownload).toHaveBeenCalledWith('owner','slimsam-77-uniform',expect.any(AbortSignal));finish(done);await vi.waitFor(()=>expect(ready).toHaveBeenCalledTimes(1));expect(cancelSpeechModelDownload).not.toHaveBeenCalled()
})
it('unmount detaches observation without cancelling download',async()=>{
 await setup();vi.mocked(observeModelDownload).mockImplementation(()=>new Promise(()=>{}));button('Скачать модель').click();await vi.waitFor(()=>expect(observeModelDownload).toHaveBeenCalledTimes(1));const signal=vi.mocked(observeModelDownload).mock.calls[0]![2];await unmount(component!);component=undefined;expect(signal.aborted).toBe(true);expect(cancelSpeechModelDownload).not.toHaveBeenCalled()
})
it('reconnects after a read failure without a second admission',async()=>{
 const ready=await setup();vi.mocked(observeModelDownload).mockRejectedValueOnce(new Error('offline')).mockResolvedValue(done);button('Скачать модель').click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('offline'));button('Обновить состояние').click();await vi.waitFor(()=>expect(ready).toHaveBeenCalledTimes(1));expect(startSamModelDownload).toHaveBeenCalledTimes(1)
})
it('surfaces cancellation failure and keeps cancellation available',async()=>{
 await setup();vi.mocked(observeModelDownload).mockImplementation((_a,_b,_c,update)=>{update({id:'job',status:'running'});return new Promise(()=>{})});vi.mocked(cancelSpeechModelDownload).mockRejectedValue(new Error('offline'));button('Скачать модель').click();await vi.waitFor(()=>expect(button('Отменить скачивание')).toBeDefined());button('Отменить скачивание').click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Не удалось отменить: offline'));expect(button('Отменить скачивание').disabled).toBe(false)
})

it('restores a historical download and paginates without starting another download',async()=>{
 const ready=await setup();vi.mocked(listSamModelDownloads).mockResolvedValueOnce({jobs:[{jobId:'old',catalogId:'base',createdAt:1,status:'running',progress:20}],nextCursor:'old'}).mockResolvedValue({jobs:[{jobId:'older',catalogId:'tiny',createdAt:0,status:'done',progress:100}],nextCursor:null});
 button('Обновить историю').click();await vi.waitFor(()=>expect(button('Ещё задания')).toBeDefined());button('Ещё задания').click();await vi.waitFor(()=>expect(listSamModelDownloads).toHaveBeenCalledWith('owner','old',expect.any(AbortSignal)));vi.mocked(observeModelDownload).mockResolvedValue(done);document.querySelector<HTMLButtonElement>('li button')!.click();await vi.waitFor(()=>expect(ready).toHaveBeenCalledTimes(1));expect(startSamModelDownload).not.toHaveBeenCalled();expect(document.querySelectorAll('li').length).toBeGreaterThan(0)
})

it('retries the same failed download and refreshes library without new admission',async()=>{
 const ready=await setup();vi.mocked(observeModelDownload).mockResolvedValueOnce({id:'job',status:'error',errorKind:'timeout',error:'network timeout'}).mockResolvedValue(done);vi.mocked(retrySpeechModelDownload).mockResolvedValue({jobId:'job',status:'pending'});button('Скачать модель').click();await vi.waitFor(()=>expect(button('Повторить скачивание')).toBeDefined());button('Повторить скачивание').click();await vi.waitFor(()=>expect(ready).toHaveBeenCalledTimes(1));expect(retrySpeechModelDownload).toHaveBeenCalledWith('owner','job',expect.any(AbortSignal));expect(startSamModelDownload).toHaveBeenCalledTimes(1)
})
it('does not offer retry for invalid downloaded bytes',async()=>{
 await setup();vi.mocked(observeModelDownload).mockResolvedValue({id:'job',status:'error',errorKind:'validation',error:'checksum mismatch'});button('Скачать модель').click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('checksum mismatch'));expect(button('Повторить скачивание')).toBeUndefined()
})
it('keeps failed retry actionable',async()=>{
 await setup();vi.mocked(observeModelDownload).mockResolvedValue({id:'job',status:'error',errorKind:'timeout'});vi.mocked(retrySpeechModelDownload).mockRejectedValue(new Error('attempt limit'));button('Скачать модель').click();await vi.waitFor(()=>expect(button('Повторить скачивание')).toBeDefined());button('Повторить скачивание').click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Не удалось повторить загрузку: attempt limit'));expect(button('Повторить скачивание').disabled).toBe(false)
})

it('suppresses late old-owner catalog and aborts its request',async()=>{
 let finish!:(value:Awaited<ReturnType<typeof listSamModels>>)=>void
 vi.mocked(listSamModelDownloads).mockResolvedValue({jobs:[],nextCursor:null})
 vi.mocked(listSamModels).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 component=mount(Harness,{target:document.body,props:{onready:vi.fn()}});await tick()
 button('Загрузить каталог моделей').click();await vi.waitFor(()=>expect(listSamModels).toHaveBeenCalledTimes(1))
 const signal=vi.mocked(listSamModels).mock.calls[0]![1]!
 button('Сменить аккаунт').click();await tick();expect(signal.aborted).toBe(true)
 finish({selectedId:'private',models:[],catalog:[{id:'private',name:'Old owner model',bytes:1}]});await tick()
 expect(document.body.textContent).not.toContain('Old owner model');expect(document.querySelector('select')).toBeNull()
 expect(startSamModelDownload).not.toHaveBeenCalled()
})

it('does not publish a late completed download to another owner',async()=>{
 let finish!:(job:Job)=>void
 vi.mocked(listSamModelDownloads).mockResolvedValue({jobs:[],nextCursor:null})
 vi.mocked(listSamModels).mockResolvedValue({selectedId:'configured',models:[],catalog:[{id:'realesr',name:'SlimSAM',bytes:1}]})
 vi.mocked(startSamModelDownload).mockResolvedValue({jobId:'job'})
 vi.mocked(observeModelDownload).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const onready=vi.fn();component=mount(Harness,{target:document.body,props:{onready}});await tick()
 button('Загрузить каталог моделей').click();await vi.waitFor(()=>expect(button('Скачать модель').disabled).toBe(false));button('Скачать модель').click()
 await vi.waitFor(()=>expect(observeModelDownload).toHaveBeenCalledTimes(1));const signal=vi.mocked(observeModelDownload).mock.calls[0]![2]
 button('Сменить аккаунт').click();await tick();finish(done);await tick()
 expect(signal.aborted).toBe(true);expect(onready).not.toHaveBeenCalled();expect(cancelSpeechModelDownload).not.toHaveBeenCalled()
 expect(document.querySelector('progress')).toBeNull()
})
