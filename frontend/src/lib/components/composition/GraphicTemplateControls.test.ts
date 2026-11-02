// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import {getGraphicTemplateValues,editGraphicTemplate,type NativeGraphicTemplate} from '$lib/api'
import {createGraphicTemplateClip} from '$lib/composition/graphicLayers'
import Editor from './GraphicTemplateControls.svelte'
vi.mock('$lib/api',()=>({editGraphicTemplate:vi.fn(),getGraphicTemplateValues:vi.fn()}))
it('routes all six control kinds through native validation and preserves vector siblings',async()=>{
 const template:NativeGraphicTemplate={id:'test',name:'Title',category:'Titles',description:'',author:'',license:'MIT',tags:[],canvas:[64,48],durationUs:1000000,graphic:{layers:[{uid:1,enabled:true,content:{kind:'text',parameters:{text:'Привет',font:'Test',opacity:80,position:[20,30],fillColor:[.1,.2,.3,.4]}}}]},controls:[{id:'text',name:'Text',kind:'text',layer:1,param:'text'},{id:'font',name:'Font',kind:'font',layer:1,param:'font'},{id:'opacity',name:'Opacity',kind:'slider',layer:1,param:'opacity',min:0,max:100},{id:'visible',name:'Visible',kind:'checkbox',layer:1,param:'enabled'},{id:'color',name:'Color',kind:'color',layer:1,param:'fill_color'},{id:'position',name:'Position',kind:'position',layer:1,param:'position'}]}
 const clip=createGraphicTemplateClip({width:64,height:48},template,'title',0)
 const onchange=vi.fn();vi.mocked(editGraphicTemplate).mockResolvedValue(template)
 const target=document.createElement('div');document.body.append(target);const component=mount(Editor,{target,props:{clip,onchange}});await tick()
 let count=0
 for(const [label,raw,expected] of [['text','مرحبا','مرحبا'],['font','Other',{family:'Other'}],['opacity','42',42],['visible','false',false],['color G','.75',[.1,.75,.3,.4]],['color HEX','#11223380','#11223380'],['position Y','-7.25',[20,-7.25]]] as const){
  const input=target.querySelector<HTMLInputElement|HTMLTextAreaElement>(`[aria-label="Шаблон ${label}"]`)!
  if(input instanceof HTMLInputElement && input.type==='checkbox')input.checked=false;else input.value=raw
  input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(onchange).toHaveBeenCalledTimes(count+1))
  expect(vi.mocked(editGraphicTemplate).mock.lastCall![1]).toBe(label.split(' ')[0]);expect(vi.mocked(editGraphicTemplate).mock.lastCall![2]).toEqual(expected)
  count++;await tick()
 }
 expect(template.graphic.layers[0]!.content).toMatchObject({parameters:{text:'Привет'}})
 await unmount(component);target.remove()
})

it('reads native animated vector values and sends clip clock while preserving sampled sibling',async()=>{
 const template:NativeGraphicTemplate={id:'user:position',name:'Position',category:'Titles',description:'',author:'',license:'MIT',tags:[],canvas:[64,48],durationUs:1000000,graphic:{layers:[{uid:1,enabled:true,originalParameters:{position:{value:{Vec2:{x:0,y:0}},keyframes:[{time:0,value:{Vec2:{x:0,y:0}},interp:'Linear'},{time:254016000000,value:{Vec2:{x:20,y:40}},interp:'Linear'}]}},content:{kind:'text',parameters:{text:'Title',position:[0,0]}}}]},controls:[{id:'position',name:'Position',kind:'position',layer:1,param:'position'}]}
 vi.mocked(getGraphicTemplateValues).mockResolvedValue({position:[15,30]})
 vi.mocked(editGraphicTemplate).mockResolvedValue(template)
 const clip=createGraphicTemplateClip({width:64,height:48},template,'title',0),onchange=vi.fn()
 const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{clip,onchange,relativeUs:500000,durationUs:1000000,animationOffsetUs:250000}})
 await vi.waitFor(()=>expect(target.querySelector<HTMLInputElement>('[aria-label="Шаблон position X"]')!.value).toBe('15'))
 await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Шаблон position Y"]')!;input.value='35';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledTimes(1))
 expect(vi.mocked(editGraphicTemplate).mock.lastCall!.slice(1)).toEqual(['position',[15,35],undefined,{relativeUs:500000,durationUs:1000000,animationOffsetUs:250000}])
 await unmount(component);target.remove()
})

it('edits sampled font style independently without overwriting the animated family',async()=>{
 const template:NativeGraphicTemplate={id:'user:font',name:'Font',category:'Titles',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:1000000,graphic:{layers:[{uid:1,enabled:true,originalParameters:{font:{value:{Text:'Base'},keyframes:[{time:0,value:{Text:'Inter'},interp:'Hold'}]}},content:{kind:'text',parameters:{text:'Title',font:'Base',fontStyle:'Regular'}}}]},controls:[{id:'font',name:'Font',kind:'font',layer:1,param:'font'}]}
 vi.mocked(getGraphicTemplateValues).mockResolvedValue({font:{family:'Inter',style:'Bold'}})
 vi.mocked(editGraphicTemplate).mockResolvedValue(template)
 const clip=createGraphicTemplateClip({width:64,height:48},template,'font',0),onchange=vi.fn(),target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{clip,onchange,relativeUs:500000,durationUs:1000000}})
 await vi.waitFor(()=>expect(target.querySelector<HTMLInputElement>('[aria-label="Шаблон font начертание"]')!.value).toBe('Bold'))
 expect(target.querySelector<HTMLInputElement>('[aria-label="Шаблон font"]')!.value).toBe('Inter')
 await tick();const input=target.querySelector<HTMLInputElement>('[aria-label="Шаблон font начертание"]')!;input.value='Italic';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(vi.mocked(editGraphicTemplate).mock.lastCall!.slice(1)).toEqual(['font',{style:'Italic'},undefined,{relativeUs:500000,durationUs:1000000,animationOffsetUs:0}])
 await unmount(component);target.remove()
})
it('renders discrete shape control as named choices and submits an integer',async()=>{
 const template:NativeGraphicTemplate={id:'user:shape',name:'Shape',category:'Shapes',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:1000000,graphic:{layers:[{uid:1,enabled:true,content:{kind:'shape',parameters:{shape:0}}}]},controls:[{id:'shape',name:'Фигура',kind:'slider',layer:1,param:'shape',min:0,max:3}]}
 vi.mocked(editGraphicTemplate).mockResolvedValue(template)
 const clip=createGraphicTemplateClip({width:64,height:48},template,'shape',0),onchange=vi.fn(),target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{clip,onchange}});await tick()
 const select=target.querySelector<HTMLSelectElement>('[aria-label="Шаблон shape"]')!
 expect(select.tagName).toBe('SELECT');expect(Array.from(select.options).map(option=>option.textContent)).toEqual(['Прямоугольник','Эллипс','Многоугольник','Путь'])
 select.value='1';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(vi.mocked(editGraphicTemplate).mock.lastCall?.[2]).toBe(1)
 await unmount(component);target.remove()
})
it('requests native sampled values for authored source text animation without original parameters',async()=>{
 const template:NativeGraphicTemplate={id:'user:authored',name:'Text',category:'Titles',description:'',author:'',license:'',tags:[],canvas:[64,48],durationUs:1000000,graphic:{layers:[{uid:1,enabled:true,content:{kind:'text',parameters:{text:'Base'},sourceTextAnimation:{timeBase:1000000,keyframes:[{tick:0,value:'A'},{tick:1000000,value:'B'}]}}}]},controls:[{id:'text',name:'Text',kind:'text',layer:1,param:'text'}]}
 vi.mocked(getGraphicTemplateValues).mockResolvedValue({text:'A'})
 vi.mocked(editGraphicTemplate).mockResolvedValue(template)
 const clip=createGraphicTemplateClip({width:64,height:48},template,'authored',0),onchange=vi.fn(),target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{clip,onchange,relativeUs:500000,durationUs:1000000}})
 await vi.waitFor(()=>expect(target.querySelector<HTMLTextAreaElement>('[aria-label="Шаблон text"]')!.value).toBe('A'))
 await tick();const input=target.querySelector<HTMLTextAreaElement>('[aria-label="Шаблон text"]')!;input.value='Edited';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(vi.mocked(editGraphicTemplate).mock.lastCall!.slice(1)).toEqual(['text','Edited',undefined,{relativeUs:500000,durationUs:1000000,animationOffsetUs:0}])
 await unmount(component);target.remove()
})
