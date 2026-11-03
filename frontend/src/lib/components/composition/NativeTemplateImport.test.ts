// @ts-expect-error Mounted tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import Control from './NativeTemplateImport.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof api>(),inspectNativeTemplatePackage:vi.fn(),importNativeTemplate:vi.fn()}))
const info:api.NativeTemplatePackageInfo={schemaVersion:1,name:'Edit',template:{schemaVersion:1,compositionId:'root',width:16,height:8,slots:[{mediaId:'b',name:'Second',kind:'video',seconds:3},{mediaId:'a',name:'First',kind:'image',seconds:2}]}}
const sources=[{label:'Chosen',source:{id:'chosen',kind:'image' as const,durationTicks:0,width:16,height:8,hasAudio:false}}]
let component:ReturnType<typeof mount>|undefined
const button=(text:string)=>[...document.querySelectorAll('button')].find(b=>b.textContent===text)!
afterEach(async()=>{if(component)await unmount(component);component=undefined;vi.resetAllMocks();document.body.innerHTML=''})
async function setup(oncreated=vi.fn().mockResolvedValue(undefined)){
 component=mount(Control,{target:document.body,props:{token:'owner',spaceId:'space',context:{},sources,oncreated}});await tick()
 const input=document.querySelector<HTMLInputElement>('input[type=file]')!,file=new File(['package'],'edit.vktemplate')
 Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 return {file,oncreated}
}
async function inspect(){button('Проверить шаблон').click();await vi.waitFor(()=>expect(document.querySelectorAll('select')).toHaveLength(2));await tick()}
async function selectAll(){for(const select of document.querySelectorAll('select')){select.value='chosen';select.dispatchEvent(new Event('change',{bubbles:true}))}await tick()}
it('inspects a chosen package and creates only after every ordered slot has a library file',async()=>{
 vi.mocked(api.inspectNativeTemplatePackage).mockResolvedValue(info)
 const result={project:{id:'saved',name:'New edit'},sourceMapping:{}} as api.NativeTemplateImportResponse;vi.mocked(api.importNativeTemplate).mockResolvedValue(result)
 const{file,oncreated}=await setup();await inspect()
 expect(api.inspectNativeTemplatePackage).toHaveBeenCalledWith(file,'owner',expect.any(AbortSignal));expect([...document.querySelectorAll('select')].map(s=>s.getAttribute('aria-label'))).toEqual(['Файл для слота Second','Файл для слота First'])
 expect(button('Создать проект из выбранного шаблона').disabled).toBe(true);await selectAll();button('Создать проект из выбранного шаблона').click()
 await vi.waitFor(()=>expect(oncreated).toHaveBeenCalledTimes(1));expect(api.importNativeTemplate).toHaveBeenCalledWith(file,[{mediaId:'b',sourceId:'chosen'},{mediaId:'a',sourceId:'chosen'}],'owner','space',expect.any(AbortSignal))
 expect(oncreated.mock.calls[0]![0]).toBe(result);expect(()=>oncreated.mock.calls[0]![1]()).not.toThrow();expect(document.body.textContent).toContain('Создан проект «New edit»');expect(document.querySelectorAll('select')).toHaveLength(0)
})
it('retains selections on server refusal and can retry without an extra inspection',async()=>{
 vi.mocked(api.inspectNativeTemplatePackage).mockResolvedValue(info);vi.mocked(api.importNativeTemplate).mockRejectedValueOnce(new Error('replacement changed')).mockResolvedValueOnce({project:{id:'saved',name:'Retry'}} as api.NativeTemplateImportResponse)
 const{oncreated}=await setup();await inspect();await selectAll();button('Создать проект из выбранного шаблона').click()
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('replacement changed'));expect(oncreated).not.toHaveBeenCalled();expect(document.querySelector('select')?.value).toBe('chosen')
 button('Создать проект из выбранного шаблона').click();await vi.waitFor(()=>expect(oncreated).toHaveBeenCalledTimes(1));expect(api.inspectNativeTemplatePackage).toHaveBeenCalledTimes(1)
})
it('aborts a pending creation on unmount and never opens its late saved project',async()=>{
 vi.mocked(api.inspectNativeTemplatePackage).mockResolvedValue(info);let resolve!:(value:api.NativeTemplateImportResponse)=>void;vi.mocked(api.importNativeTemplate).mockImplementation(()=>new Promise(done=>{resolve=done}))
 const{oncreated}=await setup();await inspect();await selectAll();button('Создать проект из выбранного шаблона').click();await vi.waitFor(()=>expect(api.importNativeTemplate).toHaveBeenCalledTimes(1))
 const signal=vi.mocked(api.importNativeTemplate).mock.calls[0]![4]!;await unmount(component!);component=undefined;expect(signal.aborted).toBe(true)
 resolve({project:{id:'late',name:'Late'}} as api.NativeTemplateImportResponse);await tick();await Promise.resolve();expect(oncreated).not.toHaveBeenCalled()
})

it('clears a private inspection and refuses its late metadata on account change',async()=>{
 const{default:Harness}=await import('./fixtures/NativeTemplateImportHarness.svelte')
 let resolve!:(value:api.NativeTemplatePackageInfo)=>void;vi.mocked(api.inspectNativeTemplatePackage).mockImplementation(()=>new Promise(done=>{resolve=done}))
 const instance=mount(Harness,{target:document.body,props:{oncreated:vi.fn()}});component=instance;await tick()
 const input=document.querySelector('input')!;Object.defineProperty(input,'files',{value:[new File(['package'],'private.vktemplate')]});input.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 button('Проверить шаблон').click();await vi.waitFor(()=>expect(api.inspectNativeTemplatePackage).toHaveBeenCalledTimes(1))
 const signal=vi.mocked(api.inspectNativeTemplatePackage).mock.calls[0]![2]!;instance.changeAccount();await tick();expect(signal.aborted).toBe(true)
 resolve(info);await tick();await Promise.resolve();expect(document.querySelectorAll('select')).toHaveLength(0);expect(document.body.textContent).not.toContain('private.vktemplate');expect(button('Проверить шаблон').disabled).toBe(true)
})
