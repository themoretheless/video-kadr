// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it,vi} from 'vitest'
import Formatting from './GraphicCharacterFormatting.svelte'
import {editGraphicTextRuns} from '$lib/api'
vi.mock('$lib/api',()=>({editGraphicTextRuns:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.resetAllMocks()})
it('sends character offsets and overrides to native formatting and preserves original runs',async()=>{
 const runs=[{start:0,end:1,style:{underline:true}}];const next=[{start:0,end:1,style:{underline:true}},{start:1,end:3,style:{fauxBold:true}}];vi.mocked(editGraphicTextRuns).mockResolvedValue(next);const onchange=vi.fn();component=mount(Formatting,{target:document.body,props:{text:'AБ🙂Z',runs,label:'Text',onchange}});await tick();
 for(const [field,value]of [['start','1'],['end','3']]){const input=document.querySelector<HTMLInputElement>(`input[aria-label="Text characters ${field}"]`)!;input.value=value!;input.dispatchEvent(new Event('input',{bubbles:true}))}
 const bold=document.querySelector<HTMLSelectElement>('select[aria-label="Text characters fauxBold"]')!;bold.value='true';bold.dispatchEvent(new Event('change',{bubbles:true}));await tick();
 [...document.querySelectorAll('button')].find(b=>b.textContent==='Применить к символам')!.click();await vi.waitFor(()=>expect(onchange).toHaveBeenCalledWith(next));expect(editGraphicTextRuns).toHaveBeenCalledWith('AБ🙂Z',runs,{kind:'apply',start:1,end:3,style:{fauxBold:true}});expect(runs).toEqual([{start:0,end:1,style:{underline:true}}]);
})
it('refuses invalid character ranges before native formatting',async()=>{
 component=mount(Formatting,{target:document.body,props:{text:'🙂',runs:[],label:'Text',onchange:vi.fn()}});await tick();const input=document.querySelector<HTMLInputElement>('input[aria-label="Text characters start"]')!;input.value='2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();[...document.querySelectorAll('button')].find(b=>b.textContent==='Применить к символам')!.click();await tick();expect(editGraphicTextRuns).not.toHaveBeenCalled();expect(document.querySelector('[role=alert]')?.textContent).toContain('диапазон');
})
