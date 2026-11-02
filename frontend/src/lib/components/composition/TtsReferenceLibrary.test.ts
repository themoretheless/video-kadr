// @ts-expect-error Tests use the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import {listLocalTtsReferences,removeLocalTtsReference} from '$lib/api'
import Library from './TtsReferenceLibrary.svelte'
vi.mock('$lib/api',()=>({listLocalTtsReferences:vi.fn(),removeLocalTtsReference:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const button=(name:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent===name)!
const sample={id:'reference',durationUs:1000000,sampleRate:24000,origin:'file' as const,available:true}
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.resetAllMocks()})
it('pages private references and selects only an available sample',async()=>{
 const onselect=vi.fn();vi.mocked(listLocalTtsReferences).mockResolvedValueOnce({references:[sample],nextCursor:'page'}).mockResolvedValueOnce({references:[{...sample,id:'revoked',origin:'clip',available:false}],nextCursor:null});component=mount(Library,{target:document.body,props:{token:'owner',selectedId:'',onselect,onremoved:vi.fn()}});await tick();button('Обновить образцы голоса').click();await vi.waitFor(()=>expect(document.querySelectorAll('option')).toHaveLength(2));button('Ещё образцы голоса').click();await vi.waitFor(()=>expect(listLocalTtsReferences).toHaveBeenCalledWith('owner','page',expect.any(AbortSignal)));const select=document.querySelector('select')!;select.value='reference';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(onselect).toHaveBeenCalledWith(sample);expect(document.querySelector<HTMLOptionElement>('option[value="revoked"]')!.disabled).toBe(true);select.value='revoked';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(onselect).toHaveBeenCalledTimes(1)
})
it.each([true,false])('removes selected sample after confirmation with filesRetained=%s',async(filesRetained)=>{
 const onremoved=vi.fn();vi.mocked(listLocalTtsReferences).mockResolvedValue({references:[sample],nextCursor:null});vi.mocked(removeLocalTtsReference).mockResolvedValue({removed:true,filesRetained});component=mount(Library,{target:document.body,props:{token:'owner',selectedId:'reference',onselect:vi.fn(),onremoved}});await tick();button('Обновить образцы голоса').click();await vi.waitFor(()=>expect(button('Убрать образец из библиотеки').disabled).toBe(false));button('Убрать образец из библиотеки').click();await vi.waitFor(()=>expect(onremoved).toHaveBeenCalledWith('reference'));expect(document.body.textContent).toContain(filesRetained?'Файлы сохранены для ранее принятых заданий':'Образец удалён.');expect(removeLocalTtsReference).toHaveBeenCalledWith('owner','reference',expect.any(AbortSignal));expect(document.querySelectorAll('option')).toHaveLength(1)
})
it('aborts a pending library read on unmount',async()=>{
 let finish!:(value:{references:typeof sample[];nextCursor:null})=>void;vi.mocked(listLocalTtsReferences).mockImplementation(()=>new Promise(done=>{finish=done}));component=mount(Library,{target:document.body,props:{token:'owner',selectedId:'',onselect:vi.fn(),onremoved:vi.fn()}});await tick();button('Обновить образцы голоса').click();await vi.waitFor(()=>expect(listLocalTtsReferences).toHaveBeenCalledTimes(1));const signal=vi.mocked(listLocalTtsReferences).mock.calls[0]![2]!;await unmount(component);component=undefined;expect(signal.aborted).toBe(true);finish({references:[sample],nextCursor:null});await tick();expect(document.querySelector('select')).toBeNull()
})
