// @ts-expect-error Mounted tests use Svelte's client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import Control from './NativeTemplateLibrary.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof api>(),listNativeTemplateLibrary:vi.fn(),installNativeTemplate:vi.fn(),removeNativeTemplate:vi.fn(),createProjectFromNativeLibrary:vi.fn()}))
const entry:api.NativeTemplateLibraryEntry={id:'stored',name:'Design',bytes:100,createdAt:1,info:{schemaVersion:1,compositionId:'root',width:16,height:8,slots:[{mediaId:'slot',name:'Picture',kind:'image',seconds:2}]}}
const sources=[{label:'Chosen',source:{id:'chosen',kind:'image' as const,durationTicks:0,width:16,height:8,hasAudio:false}}]
let component:ReturnType<typeof mount>|undefined
const button=(text:string)=>[...document.querySelectorAll('button')].find(b=>b.textContent?.includes(text))!
afterEach(async()=>{if(component)await unmount(component);component=undefined;vi.resetAllMocks();document.body.innerHTML=''})
async function setup(){const oncreated=vi.fn().mockResolvedValue(undefined);component=mount(Control,{target:document.body,props:{token:'owner',spaceId:'space',context:'document',sources,oncreated}});await tick();expect(api.listNativeTemplateLibrary).not.toHaveBeenCalled();const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(api.listNativeTemplateLibrary).toHaveBeenCalledTimes(1));await tick();return oncreated}
it('lists stored designs and creates from the chosen record without downloading or reuploading it',async()=>{
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([entry]);vi.mocked(api.createProjectFromNativeLibrary).mockResolvedValue({project:{id:'new',name:'Fresh'}} as api.NativeTemplateImportResponse)
 const oncreated=await setup();button('Design ·').click();await tick();expect(button('Создать проект из библиотечного шаблона').disabled).toBe(true)
 const select=document.querySelector('select')!;select.value='chosen';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();button('Создать проект из библиотечного шаблона').click()
 await vi.waitFor(()=>expect(oncreated).toHaveBeenCalledTimes(1));expect(api.createProjectFromNativeLibrary).toHaveBeenCalledWith('stored',[{mediaId:'slot',sourceId:'chosen'}],'owner','space',expect.any(AbortSignal));expect(api.installNativeTemplate).not.toHaveBeenCalled();expect(document.body.textContent).toContain('Создан проект «Fresh»')
})
it('installs a selected file and removes the stored record through owner APIs',async()=>{
 vi.mocked(api.listNativeTemplateLibrary).mockResolvedValue([]);vi.mocked(api.installNativeTemplate).mockResolvedValue(entry);vi.mocked(api.removeNativeTemplate).mockResolvedValue(undefined)
 await setup();const input=document.querySelector('input')!,file=new File(['package'],'design.vktemplate');Object.defineProperty(input,'files',{value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}));await tick();button('Сохранить пакет в библиотеку').click()
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Шаблон добавлен'));expect(api.installNativeTemplate).toHaveBeenCalledWith(file,'owner',expect.any(AbortSignal));expect(document.querySelector('a')?.getAttribute('href')).toBe('/api/template-library/stored')
 document.querySelector<HTMLButtonElement>('button[aria-label="Удалить шаблон Design"]')!.click();await vi.waitFor(()=>expect(api.removeNativeTemplate).toHaveBeenCalledWith('stored','owner',expect.any(AbortSignal)));await vi.waitFor(()=>expect(document.querySelector('a')).toBeNull())
})
it('aborts library observation on collapse and ignores a late private list',async()=>{
 let resolve!:(value:api.NativeTemplateLibraryEntry[])=>void;vi.mocked(api.listNativeTemplateLibrary).mockImplementation(()=>new Promise(done=>{resolve=done}));await setup();const signal=vi.mocked(api.listNativeTemplateLibrary).mock.calls[0]![1]!
 const details=document.querySelector('details')!;details.open=false;details.dispatchEvent(new Event('toggle'));await tick();expect(signal.aborted).toBe(true);resolve([entry]);await tick();await Promise.resolve();expect(document.body.textContent).not.toContain('Design ·')
})

it('drops the previous owner list on account change before accepting a new one',async()=>{
 const{default:Harness}=await import('./fixtures/NativeTemplateLibraryHarness.svelte');const pending:Array<(entries:api.NativeTemplateLibraryEntry[])=>void>=[]
 vi.mocked(api.listNativeTemplateLibrary).mockImplementation(()=>new Promise(done=>pending.push(done)))
 const instance=mount(Harness,{target:document.body});component=instance;await tick();const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(pending).toHaveLength(1));const old=vi.mocked(api.listNativeTemplateLibrary).mock.calls[0]![1]!
 instance.changeAccount();await tick();await vi.waitFor(()=>expect(pending).toHaveLength(2));expect(old.aborted).toBe(true);pending[0]!([entry]);await tick();expect(document.body.textContent).not.toContain('Design ·');pending[1]!([]);await tick();expect(document.body.textContent).not.toContain('Design ·');expect(vi.mocked(api.listNativeTemplateLibrary).mock.calls[1]![0]).toBe('other')
})
