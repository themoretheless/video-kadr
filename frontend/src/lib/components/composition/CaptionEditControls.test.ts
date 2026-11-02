// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Controls from './CaptionEditControls.svelte'
import {createComposition} from '$lib/composition/commands'
import {buildCompositionWireDocument} from '$lib/composition/payload'
import {editCompositionCaptions} from '$lib/api'
import type {Composition} from '$lib/composition/types'
vi.mock('$lib/api',()=>({editCompositionCaptions:vi.fn()}))
function fixture():Composition{return {...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Text',speaker:'Ann'}]}]}}
it('submits a native split and commits both halves once, selecting the new right caption',async()=>{
 const composition=fixture(),onchange=vi.fn(),onselection=vi.fn(),target=document.createElement('div')
 const cue=composition.captionTracks![0].clips[0]
 const after:Composition={...composition,captionTracks:[{...composition.captionTracks![0],clips:[{...cue,timelineEndTick:1000000},{...cue,id:'right',timelineStartTick:1000000}]}]}
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument(after),appliedDelta:null,deleted:0})
 const component=mount(Controls,{target,props:{composition,selected:['cue'],playheadTicks:1000000,onchange,onselection}});await tick()
 ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Разрезать субтитр')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),{kind:'split',id:'cue',timeTick:1000000})
 expect(onchange.mock.calls[0][0].captionTracks[0].clips).toHaveLength(2);expect(onselection).toHaveBeenLastCalledWith(['right'])
 expect(composition.captionTracks![0].clips).toHaveLength(1)
 await unmount(component)
})
it('sends explicit null to clear a caption speaker',async()=>{
 const composition=fixture(),onchange=vi.fn(),target=document.createElement('div')
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument({...composition,captionTracks:[{...composition.captionTracks![0],clips:[{...composition.captionTracks![0].clips[0],speaker:null}]}]}),appliedDelta:null,deleted:0})
 const component=mount(Controls,{target,props:{composition,selected:['cue'],playheadTicks:0,onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Спикер выбранного субтитра"]')!;input.value='';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Применить спикера')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),{kind:'text',id:'cue',speaker:null})
 expect(onchange.mock.calls[0][0].captionTracks[0].clips[0].speaker).toBeNull()
 await unmount(component)
})

it('preserves the untouched exact endpoint when editing only the other caption boundary',async()=>{
 const base=fixture(),composition:Composition={...base,captionTracks:[{...base.captionTracks![0],clips:[{...base.captionTracks![0].clips[0],timelineStartTick:'137729592000',timelineEndTick:'508032000001'}]}]}
 const target=document.createElement('div'),onchange=vi.fn()
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument(composition),appliedDelta:null,deleted:0})
 const component=mount(Controls,{target,props:{composition,selected:['cue'],playheadTicks:0,onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Начало выбранного субтитра"]')!
 input.value='0.75';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Применить время субтитра')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),{kind:'times',id:'cue',startTick:750000})
 await unmount(component)
})

it('submits an exact thirteen-frame shift at 23.976 without rounding through microseconds',async()=>{
 const base=fixture(),composition:Composition={...base,canvas:{...base.canvas,fps:24000/1001,fpsRate:{numerator:24000,denominator:1001}}}
 const target=document.createElement('div'),onchange=vi.fn();document.body.append(target)
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument(composition),appliedDelta:'137729592000',deleted:0})
 const component=mount(Controls,{target,props:{composition,selected:['cue'],playheadTicks:0,onchange}});await tick()
 const unit=target.querySelector<HTMLSelectElement>('[aria-label="Единицы сдвига субтитров"]')!
 unit.value='frames';unit.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 await vi.waitFor(()=>expect(target.querySelector('[aria-label="Сдвиг субтитров в кадрах"]')).not.toBeNull())
 const input=target.querySelector<HTMLInputElement>('[aria-label="Сдвиг субтитров в кадрах"]')!
 input.value='13';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Сдвинуть субтитры')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),{kind:'move',ids:['cue'],deltaTicks:'137729592000'})
 await unmount(component);target.remove()
})

it.each(['frames','timecode'] as const)('authors only the changed end in %s with exact native clock context',async(unitName)=>{
 const composition=fixture(),target=document.createElement('div'),onchange=vi.fn()
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument(composition),appliedDelta:null,deleted:0})
 const component=mount(Controls,{target,props:{composition,selected:['cue'],playheadTicks:1000000,onchange}});await tick()
 const unit=target.querySelector<HTMLSelectElement>('[aria-label="Единицы времени субтитра"]')!
 unit.value=unitName;unit.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const end=target.querySelector<HTMLInputElement>('[aria-label="Точный конец выбранного субтитра"]')!
 end.value=unitName==='frames'?'14':'+15';end.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 if(unitName==='timecode'){
  const drop=target.querySelector<HTMLInputElement>('[aria-label="Drop-frame таймкод субтитра"]')!
  drop.checked=true;drop.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 }
 ;[...target.querySelectorAll('button')].find(button=>button.textContent==='Применить время субтитра')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 const operation={kind:'times',id:'cue',endInput:{unit:unitName,value:unitName==='frames'?14:'+15'}}
 if(unitName==='timecode')expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),operation,undefined,{playheadTick:1000000,dropFrame:true})
 else expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),operation)
 await unmount(component)
})
