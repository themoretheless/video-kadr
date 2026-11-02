// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {it,expect,vi} from 'vitest'
import {exportGraphicTemplateFile,type NativeGraphicTemplate} from '$lib/api'
import {createGraphicClip,createGraphicTemplateClip} from '$lib/composition/graphicLayers'
import Export from './GraphicTemplateExport.svelte'
vi.mock('$lib/api',()=>({exportGraphicTemplateFile:vi.fn()}))
it('exports authored layers, retained keys and font resources with clip trim offset and surfaces native conversion refusal',async()=>{
 const template:NativeGraphicTemplate={id:'user:title',name:'Title',category:'Titles',description:'',author:'',license:'MIT',tags:[],canvas:[64,48],durationUs:2000000,controls:[],resources:{fonts:[{family:'Test',style:'Regular',license:'OFL-1.1',file:'test.ttf',data:'TWFu'}]},graphic:{layers:[{uid:1,enabled:true,originalParameters:{opacity:{value:{Float:100},keyframes:[{time:0,value:{Float:0},interp:'Linear'}]}},content:{kind:'text',parameters:{text:'Привет'}}}]}}
 const clip={...createGraphicTemplateClip({width:64,height:48},template,'title',0),animationOffsetUs:250000}
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('Native curve cannot be represented'))
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Название экспортируемого шаблона"]')!;input.value='Edited title';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(target.querySelector('[role="alert"]')?.textContent).toContain('Native curve cannot be represented'))
 expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall).toEqual([{...template,name:'Edited title',roll:null,responsiveTime:null},250000])
 expect(clip.template?.name).toBe('Title')
 const embed=target.querySelector<HTMLInputElement>('[aria-label="Встроить шрифты в шаблон"]')!;embed.checked=true;embed.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const license=target.querySelector<HTMLInputElement>('[aria-label="Лицензия встраиваемых шрифтов"]')!;license.value='OFL-test';license.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.slice(1)).toEqual([250000,undefined,{embedFonts:true,fontLicense:'OFL-test'}]))

 await unmount(component);target.remove()
})
it('authors named numeric controls with limits without mutating clip metadata',async()=>{
 const template:NativeGraphicTemplate={id:'user:shape',name:'Shape',category:'Shapes',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:2000000,controls:[],graphic:{layers:[{uid:7,enabled:true,content:{kind:'shape',parameters:{opacity:75,size:[20,30],fillColor:[1,0,0,1]}}}]}}
 const clip=createGraphicTemplateClip({width:64,height:48},template,'shape',0)
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('stop download'))
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 const select=target.querySelector<HTMLSelectElement>('select')!;select.value='7:opacity';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 await tick();Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent==='Добавить контрол')!.click();await tick();expect(target.innerHTML).toContain('Название контрола 1')
 for(const [label,value] of [['Название контрола 1','Прозрачность'],['Минимум контрола 1','0'],['Максимум контрола 1','100']]){const input=target.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;input.value=value!;input.dispatchEvent(new Event('input',{bubbles:true}));await tick()}
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.[0].controls).toEqual([{id:'control_1',name:'Прозрачность',kind:'slider',layer:7,param:'opacity',min:0,max:100}]))
 expect(clip.template?.controls).toEqual([])
 target.querySelector<HTMLButtonElement>('[aria-label="Удалить контрол 1"]')!.click();await tick();expect(target.querySelector('[aria-label="Название контрола 1"]')).toBeNull()
 await unmount(component);target.remove()
})

it('offers omitted default font, automatic position and color parameters on a fresh text clip',async()=>{
 const clip=createGraphicClip({width:64,height:48},'fresh',0,1000000,'text')
 const before=JSON.stringify(clip)
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 const select=target.querySelector<HTMLSelectElement>('select')!
 const options=Array.from(select.options).map(option=>option.value)
 expect(options).toEqual(expect.arrayContaining(['1:font','1:font_style','1:position','1:fill_color','1:kerning','1:tracking']))
 expect(options.filter(value=>value==='1:fill_color')).toHaveLength(1)
 select.value='1:font';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent==='Добавить контрол')!.click();await tick()
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('stop download'))
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.[0].controls).toEqual([{id:'control_1',name:'Слой 1 · Шрифт',kind:'font',layer:1,param:'font'}]))
 expect(JSON.stringify(clip)).toBe(before)
 await unmount(component);target.remove()
})

it('reorders control identities and limits in exported copy while preserving source order',async()=>{
 const controls=[{id:'first',name:'Opacity',kind:'slider' as const,layer:7,param:'opacity',min:0,max:100},{id:'second',name:'Size',kind:'position' as const,layer:7,param:'size'}]
 const template:NativeGraphicTemplate={id:'user:order',name:'Order',category:'Shapes',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:1000000,controls,graphic:{layers:[{uid:7,enabled:true,content:{kind:'shape',parameters:{opacity:75,size:[20,30]}}}]}}
 const clip=createGraphicTemplateClip({width:64,height:48},template,'order',0),before=JSON.stringify(clip)
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('stop download'))
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 expect(target.querySelector<HTMLButtonElement>('[aria-label="Контрол 1 выше"]')!.disabled).toBe(true)
 expect(target.querySelector<HTMLButtonElement>('[aria-label="Контрол 2 ниже"]')!.disabled).toBe(true)
 target.querySelector<HTMLButtonElement>('[aria-label="Контрол 2 выше"]')!.click();await tick()
 expect(target.querySelector<HTMLInputElement>('[aria-label="Название контрола 1"]')!.value).toBe('Size')
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.[0].controls).toEqual([controls[1],controls[0]]))
 expect(JSON.stringify(clip)).toBe(before)
 await unmount(component);target.remove()
})

it('authors explicit control identity and kind and refuses duplicate or blank identities locally',async()=>{
 const controls=[{id:'first',name:'Opacity',kind:'slider' as const,layer:7,param:'opacity'},{id:'second',name:'Name',kind:'text' as const,layer:7,param:'name'}]
 const template:NativeGraphicTemplate={id:'user:custom',name:'Custom',category:'Shapes',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:1000000,controls,graphic:{layers:[{uid:7,enabled:true,content:{kind:'shape',parameters:{opacity:75,name:'Shape'}}}]}}
 const clip=createGraphicTemplateClip({width:64,height:48},template,'custom',0),before=JSON.stringify(clip)
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('stop download'))
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 const id=target.querySelector<HTMLInputElement>('[aria-label="Идентификатор контрола 1"]')!
 for(const invalid of ['second',' ']){id.value=invalid;id.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(id.value).toBe('first');expect(target.querySelector('[role="alert"]')?.textContent).toContain('уникальным')}
 id.value='custom_opacity';id.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const kind=target.querySelector<HTMLSelectElement>('[aria-label="Тип контрола 1"]')!;expect(Array.from(kind.options).map(option=>option.value)).toEqual(['text','font','slider','checkbox','color','position'])
 kind.value='text';kind.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.[0].controls[0]).toEqual({...controls[0],id:'custom_opacity',kind:'text'}))
 expect(JSON.stringify(clip)).toBe(before)
 await unmount(component);target.remove()
})

it('exports intentionally empty controls when automatic text controls are disabled',async()=>{
 const clip=createGraphicClip({width:64,height:48},'empty-controls',0,1000000,'text')
 const before=JSON.stringify(clip)
 vi.mocked(exportGraphicTemplateFile).mockRejectedValue(new Error('stop download'))
 const target=document.createElement('div');document.body.append(target);const component=mount(Export,{target,props:{clip}});await tick()
 const checkbox=target.querySelector<HTMLInputElement>('[aria-label="Автоматически добавить текстовые контролы"]')!;expect(checkbox.checked).toBe(true)
 checkbox.checked=false;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent?.includes('Скачать графику'))!.click()
 await vi.waitFor(()=>expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.slice(1)).toEqual([0,undefined,{autoTextControls:false}]))
 expect(vi.mocked(exportGraphicTemplateFile).mock.lastCall?.[0].controls).toEqual([])
 expect(JSON.stringify(clip)).toBe(before)
 await unmount(component);target.remove()
})
