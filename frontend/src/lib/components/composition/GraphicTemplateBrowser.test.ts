// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {it,expect,vi} from 'vitest'
import {getGraphicTemplates,type NativeGraphicTemplate} from '$lib/api'
import Browser from './GraphicTemplateBrowser.svelte'
vi.mock('$lib/api',()=>({getGraphicTemplateThumbnail:vi.fn().mockResolvedValue({width:16,height:9,pngBase64:'AA=='}),getGraphicTemplateDocumentThumbnail:vi.fn().mockResolvedValue({width:16,height:9,pngBase64:'AA=='}),getGraphicTemplates:vi.fn(),listUserGraphicTemplates:vi.fn(),installUserGraphicTemplate:vi.fn(),removeUserGraphicTemplate:vi.fn(),writeGraphicTemplateFile:vi.fn(),instantiateGraphicTemplateFile:vi.fn()}))
it('selects matching template and disables insertion when category and search have no intersection',async()=>{
 const base={description:'',author:'',license:'',canvas:[64,48],durationUs:1000000,graphic:{layers:[]},controls:[]} as const
 const templates:NativeGraphicTemplate[]=[{...base,id:'a',name:'Title',category:'Titles',tags:['news']},{...base,id:'b',name:'Logo',category:'Callouts',tags:['brand']}]
 vi.mocked(getGraphicTemplates).mockResolvedValue(templates)
 const target=document.createElement('div');document.body.append(target);const onchoose=vi.fn(),component=mount(Browser,{target,props:{onchoose}});await tick()
 target.querySelector<HTMLButtonElement>('button')!.click()
 await vi.waitFor(()=>expect(target.querySelectorAll('[aria-label="Графический шаблон"] option')).toHaveLength(2));await tick()
 const query=target.querySelector<HTMLInputElement>('[aria-label="Поиск встроенных шаблонов"]')!;query.value='BRAND';query.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const add=Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(button=>button.textContent==='Добавить шаблон')!
 expect(target.querySelectorAll('[aria-label="Графический шаблон"] option')).toHaveLength(1);add.click();expect(onchoose).toHaveBeenLastCalledWith(templates[1])
 const category=target.querySelector<HTMLSelectElement>('[aria-label="Категория встроенных шаблонов"]')!;category.value='Titles';category.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(add.disabled).toBe(true);expect(target.querySelector('[role="status"]')?.textContent).toBe('Шаблоны не найдены')
 query.value='';query.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(add.disabled).toBe(false);add.click();expect(onchoose).toHaveBeenLastCalledWith(templates[0])
 await unmount(component);target.remove()
})
