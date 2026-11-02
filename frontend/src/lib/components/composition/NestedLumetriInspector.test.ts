// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { removeProjectLut, updateProjectLut, exportColorLut, importProjectLut, listBuiltinLuts, renderNestedPreview, solveColorMatch, sampleToneCurve, validateColorLut, evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComparison, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedLumetriInspector.svelte'
vi.mock('$lib/api',()=>({removeProjectLut:vi.fn(),updateProjectLut:vi.fn(),exportColorLut:vi.fn(),importProjectLut:vi.fn(),evaluateNestedFrame:vi.fn(),validateColorLut:vi.fn(),sampleToneCurve:vi.fn(),renderNestedPreview:vi.fn(),solveColorMatch:vi.fn(),listBuiltinLuts:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 vi.mocked(listBuiltinLuts).mockResolvedValue([{id:"slog3-sgamut3cine-to-rec709",label:"Sony SLog3",input:true,size:33},{id:"look-night",label:"Night",input:false,size:33}])
 vi.mocked(sampleToneCurve).mockResolvedValue(Array.from({length:256},(_,i)=>i/255))
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{canvas:{fpsMilli:25000},tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000,timelineDurationTick:1000000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(button).toBeDefined();button!.click()}
async function input(label:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
function parameters(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected');const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='lumetri')throw new Error('grade expected');return effect.parameters}
it('authors correction and clip-local animation through validated undo history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelectorAll('input[type="number"]')).toHaveLength(46)
 await input('clip Lumetri basic exposure','-3',2)
 expect(parameters().basic?.exposure).toBe(-3)
 expect(nestedComposition.renderedOutputs).toEqual({})
 const key=document.querySelector<HTMLButtonElement>('button[aria-label="clip Lumetri basic exposure ключ"]')!
 expect(key.disabled).toBe(true)
 nestedComposition.timeUs=600000;await tick();key.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(track.kind!=='video')throw new Error('video')
 const effect=track.clips[0]!.effects[0]!
 if(effect.kind!=='lumetri')throw new Error('Lumetri')
 expect(effect.parameters.animation?.basic?.exposure).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:-3}]}})
 click('Сбросить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().basic?.exposure).toBe(-3)
})

it('authors wheel coordinates and their local keys without changing other sections',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('clip Lumetri grade shadowsX','0.7',2)
 expect(parameters().grade?.shadows?.x).toBe(0.7)
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Lumetri grade shadowsX ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
 expect(parameters().animation?.grade?.shadowsX).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:0.7}]}})
 undoNestedComposition();expect(parameters().animation).toBeUndefined()
 expect(parameters().grade?.shadows?.x).toBe(0.7)
})

it('validates and assigns creative LUT without replacing the unified effect',async()=>{
 vi.mocked(validateColorLut).mockResolvedValue({title:'Identity',shaperSize:2,cubeSize:0})
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('clip Lumetri basic exposure','1.5',2)
 const field=document.querySelector<HTMLInputElement>('input[aria-label="clip Lumetri lookLut импорт"]')!
 const text='LUT_1D_SIZE 2\n0 0 0\n1 1 1\n'
 const file=new File([text],'identity.cube');Object.defineProperty(file,'text',{value:async()=>text})
 Object.defineProperty(field,'files',{configurable:true,value:[file]});field.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
 expect(validateColorLut).toHaveBeenCalled()
 expect(parameters().lookLut).toEqual({format:'cube',text,intensity:100})
 expect(parameters().basic?.exposure).toBe(1.5)
 undoNestedComposition();expect(parameters().lookLut).toBeUndefined()
})

it('authors RGB and periodic HSL curves within unified history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 await input('Общая точка 2 выход','0.7',3)
 expect(parameters().curves?.master).toEqual([[0,0],[0.5,0.7],[1,1]])
 const channel=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri канал кривой"]')!
 channel.value='hueVsSat';channel.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4))
 expect(parameters().curves?.hueVsSat).toEqual([[0,0.5],[0.5,0.5]])
 expect(parameters().curves?.master?.[1]).toEqual([0.5,0.7])
 expect(vi.mocked(sampleToneCurve).mock.calls.some(call=>call[2]===true)).toBe(true)
 undoNestedComposition();expect(parameters().curves?.hueVsSat).toBeUndefined()
})

it('sets the HSL mask view while preserving correction and undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('clip Lumetri secondary hue','230',2)
 const view=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri HSL просмотр"]')!
 view.value='3';view.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
 expect(parameters().secondary).toEqual({hue:230,showMask:3})
 undoNestedComposition();expect(parameters().secondary).toEqual({hue:230})
})

it('fetches reference and probe then applies match in one undo operation',async()=>{
 nestedComposition.document={...nestedComposition.document!,luts:[{id:'shared',name:'Shared',lut:{format:'cube',text:'LUT_1D_SIZE 2\n0 0 0\n1 1 1\n',intensity:100}}]}
 vi.mocked(renderNestedPreview).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray([100,120,140,255])})
 vi.mocked(solveColorMatch).mockResolvedValue({shadows:[0.1,0.2],midtones:[0,0],highlights:[0,0],lightness:[10,0,0],saturation:120,before:0.2,after:0.01})
 component=mount(Inspector,{target:document.body,props:{token:'session'}});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const ref=document.querySelector<HTMLInputElement>('input[aria-label="clip Lumetri эталон"]')!
 ref.value='1200000';ref.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 nestedComposition.timeUs=600000;await tick();click('Применить Color Match')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
 expect(renderNestedPreview).toHaveBeenCalledTimes(2)
 expect(vi.mocked(solveColorMatch).mock.calls[0]?.[5]).toEqual(nestedComposition.document!.luts)
 expect(parameters().grade?.shadows).toEqual({x:0.1,y:0.2,lightness:10})
 expect(parameters().basic?.saturation).toBe(120)
 undoNestedComposition();expect(parameters()).toEqual({})
})
it('rejects a reference inside the matched clip before fetching frames',async()=>{
 component=mount(Inspector,{target:document.body,props:{token:'session'}});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const ref=document.querySelector<HTMLInputElement>('input[aria-label="clip Lumetri эталон"]')!
 ref.value='600000';ref.dispatchEvent(new Event('input',{bubbles:true}));await tick();click('Применить Color Match')
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('другого клипа'))
 expect(renderNestedPreview).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(1)
})

it('shares comparison reference time without adding an authored undo entry',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const comparison=document.querySelector<HTMLInputElement>('input[aria-label="Lumetri сравнение кадров"]')!
 comparison.checked=true;comparison.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const ref=document.querySelector<HTMLInputElement>('input[aria-label="clip Lumetri эталон"]')!
 ref.value='1200000';ref.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(nestedComparison).toEqual({enabled:true,referenceUs:1200000})
 expect(nestedHistory.past).toHaveLength(1)
})

it('sets and steps the reference from the inspector without grade edits',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 nestedComposition.timeUs=12345;await tick();click('Текущий кадр как эталон');await tick()
 expect(nestedComparison.referenceUs).toBe(12345)
 click('Следующий эталонный кадр');await tick();expect(nestedComparison.referenceUs).toBe(52345)
 click('Предыдущий эталонный кадр');await tick();expect(nestedComparison.referenceUs).toBe(12345)
 expect(nestedHistory.past).toHaveLength(1)
})

it('selects a builtin LUT while preserving basic correction and supports undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('clip Lumetri basic exposure','1.5',2)
 await vi.waitFor(()=>expect(document.querySelectorAll('select[aria-label="clip Lumetri inputLut встроенный"] option')).toHaveLength(2))
 const field=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri inputLut встроенный"]')!
 field.value='slog3-sgamut3cine-to-rec709';field.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
 expect(parameters().inputLutBuiltin).toBe('slog3-sgamut3cine-to-rec709')
 expect(parameters().inputLut).toBeNull();expect(parameters().basic?.exposure).toBe(1.5)
 undoNestedComposition();expect(parameters().inputLutBuiltin).toBeUndefined()
})

it('selects a shared project LUT and switches to builtin with undo',async()=>{
 nestedComposition.document={...nestedComposition.document!,luts:[{id:'shared',name:'Shared look',lut:{format:'cube',text:'LUT_1D_SIZE 2\n0 0 0\n1 1 1\n',intensity:100}}]}
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const library=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri lookLut библиотека"]')!
 expect(library.querySelector('option[value="shared"]')?.textContent).toBe('Shared look')
 library.value='shared';library.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(parameters().lookLutLibrary).toBe('shared')
 expect(parameters().lookLut).toBeNull();expect(parameters().lookLutBuiltin).toBeNull()
 const builtin=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri lookLut встроенный"]')!
 await vi.waitFor(()=>expect(builtin.querySelector('option[value="look-night"]')).not.toBeNull())
 builtin.value='look-night';builtin.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3))
 expect(parameters().lookLutLibrary).toBeNull();expect(parameters().lookLutBuiltin).toBe('look-night')
 expect(nestedComposition.document!.luts).toHaveLength(1)
 undoNestedComposition();expect(parameters().lookLutLibrary).toBe('shared')
})

it('imports into the project library and assigns it as a single undo edit',async()=>{
 vi.mocked(validateColorLut).mockResolvedValue({title:'Identity',shaperSize:2,cubeSize:0})
 vi.mocked(importProjectLut).mockImplementation(async(project,lut,name,sourcePath,composition,trackId,clipId,index,input)=>{
  const changed=structuredClone(project)
  changed.luts=[{id:'lut1',name,sourcePath,lut}]
  const track=changed.compositions[composition]!.content.tracks.find(t=>t.id===trackId)!
  if(track.kind!=='video')throw new Error('video')
  const clip=track.clips.find(c=>c.id===clipId)!
  const updated=clip.effects.map((e,i)=>i===index&&e.kind==='lumetri'?{...e,parameters:{...e.parameters,[input?'inputLutLibrary':'lookLutLibrary']:'lut1'}}:e)
  Object.assign(clip,{effects:updated})
  return {project:changed,id:'lut1',reused:false}
 })
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const field=document.querySelector<HTMLInputElement>('input[aria-label="clip Lumetri lookLut импорт в библиотеку"]')!
 const text='LUT_1D_SIZE 2\n0 0 0\n1 1 1\n'
 const file=new File([text],'identity.cube');Object.defineProperty(file,'text',{value:async()=>text})
 Object.defineProperty(field,'files',{configurable:true,value:[file]});field.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
 expect(parameters().lookLutLibrary).toBe('lut1')
 expect(nestedComposition.document!.luts?.[0]?.name).toBe('identity')
 expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document!.luts).toBeUndefined();expect(parameters().lookLutLibrary).toBeUndefined()
 redoNestedComposition();expect(parameters().lookLutLibrary).toBe('lut1')
})

it('exports a selected project LUT through the native API without editing history',async()=>{
 nestedComposition.document={...nestedComposition.document!,luts:[{id:'shared',name:'Shared',lut:{format:'cube',text:'LUT_1D_SIZE 2\n0 0 0\n1 1 1\n',intensity:100}}]}
 vi.mocked(exportColorLut).mockResolvedValue({text:'LUT_1D_SIZE 2\n0 0 0\n1 1 1\n',filename:'Identity.cube'})
 const create=vi.fn(()=> 'blob:lut-export'),revoke=vi.fn()
 vi.stubGlobal('URL',{createObjectURL:create,revokeObjectURL:revoke})
 const clickDownload=vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(function(this:HTMLAnchorElement){expect(this.download).toBe('Identity.cube')})
 try {
  component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
  const library=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri lookLut библиотека"]')!
  library.value='shared';library.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
  document.querySelector<HTMLButtonElement>('button[aria-label="clip Lumetri lookLut экспорт cube"]')!.click()
  await vi.waitFor(()=>expect(clickDownload).toHaveBeenCalledTimes(1))
  expect(vi.mocked(exportColorLut).mock.calls[0]?.[0]).toEqual({reference:'lib:shared',luts:nestedComposition.document!.luts})
  expect(vi.mocked(exportColorLut).mock.calls[0]?.[1]).toBe('cube')
  expect(create).toHaveBeenCalledTimes(1);await vi.waitFor(()=>expect(revoke).toHaveBeenCalledWith('blob:lut-export'))
  expect(nestedHistory.past).toHaveLength(2)
 }finally{clickDownload.mockRestore();vi.unstubAllGlobals()}
})

it('renames and replaces a shared LUT with undo and render invalidation',async()=>{
 const identity='LUT_1D_SIZE 2\n0 0 0\n1 1 1\n'
 nestedComposition.document={...nestedComposition.document!,luts:[{id:'shared',name:'Identity',lut:{format:'cube',text:identity,intensity:100}}]}
 vi.mocked(updateProjectLut).mockImplementation(async(project,id,changes)=>({...project,luts:project.luts!.map(l=>l.id===id?{...l,...changes}:l)}))
 component=mount(Inspector,{target:document.body});await tick()
 const name=document.querySelector<HTMLInputElement>('input[aria-label="LUT shared имя"]')!
 name.value='Renamed';name.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(nestedComposition.document!.luts?.[0]?.name).toBe('Renamed');expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 const field=document.querySelector<HTMLInputElement>('input[aria-label="LUT shared заменить"]')!
 const inverse='LUT_1D_SIZE 2\n1 1 1\n0 0 0\n'
 const file=new File([inverse],'inverse.cube');Object.defineProperty(file,'text',{value:async()=>inverse})
 Object.defineProperty(field,'files',{configurable:true,value:[file]});field.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
 expect(nestedComposition.document!.luts?.[0]).toMatchObject({id:'shared',name:'Renamed',sourcePath:'inverse.cube',lut:{text:inverse}})
 expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document!.luts?.[0]?.lut.text).toBe(identity);expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 undoNestedComposition();expect(nestedComposition.document!.luts?.[0]?.name).toBe('Identity')
})

it('removes a shared LUT, displays its orphan reference and restores both by undo',async()=>{
 nestedComposition.document={...nestedComposition.document!,luts:[{id:'shared',name:'Shared',lut:{format:'cube',text:'LUT_1D_SIZE 2\n0 0 0\n1 1 1\n',intensity:100}}]}
 vi.mocked(removeProjectLut).mockImplementation(async(project,id)=>({...project,luts:project.luts!.filter(l=>l.id!==id)}))
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Lumetri');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const library=document.querySelector<HTMLSelectElement>('select[aria-label="clip Lumetri lookLut библиотека"]')!
 library.value='shared';library.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="LUT shared удалить из библиотеки"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 expect(nestedComposition.document!.luts).toEqual([]);expect(parameters().lookLutLibrary).toBe('shared')
 expect(document.querySelector('select[aria-label="clip Lumetri lookLut библиотека"] option[value="shared"]')?.textContent).toContain('Таблица удалена')
 undoNestedComposition();await tick();expect(nestedComposition.document!.luts).toHaveLength(1);expect(parameters().lookLutLibrary).toBe('shared')
})
