// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Captions from './CompositionCaptions.svelte'
import {createComposition} from '$lib/composition/commands'
import {createGraphicClip} from '$lib/composition/graphicLayers'
import {buildCompositionWireDocument} from '$lib/composition/payload'
import {writeCaptionFile,upgradeCompositionCaptions,editCompositionCaptions} from '$lib/api'
import type {Composition} from '$lib/composition/types'
vi.mock('$lib/api',()=>({upgradeCompositionCaptions:vi.fn(),editCompositionCaptions:vi.fn(),writeCaptionFile:vi.fn(),prepareCaptionFileImport:vi.fn()}))
it('selects captions, calls native upgrade, and commits editable graphics without changing timing',async()=>{
 const document:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',clips:[{id:'cue',timelineStartTick:123,timelineEndTick:2000123,text:'Hello'}]}]}
 const graphic=createGraphicClip(document.canvas,'converted',123,2000000,'text')
 const after=buildCompositionWireDocument({...document,captionTracks:[{...document.captionTracks![0],clips:[]}],tracks:[{kind:'graphic',id:'graphics',name:'Captions as Graphics',hidden:false,locked:false,clips:[graphic]}]})
 vi.mocked(upgradeCompositionCaptions).mockResolvedValue(after)
 const target=globalThis.document.createElement('div')
 const onchange=vi.fn();const component=mount(Captions,{target,props:{composition:document,onchange}});await tick()
 const checkbox=target.querySelector<HTMLInputElement>('input[aria-label="Выбрать субтитр cue"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const button=[...target.querySelectorAll('button')].find(button=>button.textContent?.includes('Преобразовать'))!;expect(button.disabled).toBe(false);button.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(upgradeCompositionCaptions).toHaveBeenCalledWith(buildCompositionWireDocument(document),['cue'])
 const next=onchange.mock.calls[0][0] as Composition
 expect(next.captionTracks![0].clips).toEqual([]);expect(next.tracks[0].clips[0]).toMatchObject({kind:'graphic',timelineStartTicks:123,durationTicks:2000000})
 expect(document.captionTracks![0].clips).toHaveLength(1)
 await unmount(component)
})
it('adds in sorted order and cuts the default duration at the next caption',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',clips:[{id:'later',timelineStartTick:1000000,timelineEndTick:2000000,text:'Later'}]}]}
 const after={...composition,captionTracks:[{...composition.captionTracks![0],clips:[{id:'added',timelineStartTick:0,timelineEndTick:1000000,text:'Субтитр'},...composition.captionTracks![0].clips]}]}
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument(after),appliedDelta:null,deleted:0})
 const target=globalThis.document.createElement('div'),onchange=vi.fn();const component=mount(Captions,{target,props:{composition,playheadTicks:0,onchange}});await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 expect(editCompositionCaptions).toHaveBeenLastCalledWith(buildCompositionWireDocument(composition),{kind:'add',trackId:'captions',startTick:0,text:'Субтитр'})
 const next=onchange.mock.calls[0][0] as Composition
 expect(next.captionTracks![0].clips.map(cue=>[cue.timelineStartTick,cue.timelineEndTick])).toEqual([[0,1000000],[1000000,2000000]])
 expect(composition.captionTracks![0].clips).toHaveLength(1)
 await unmount(component)
})
it('refuses adding over an existing caption without committing a document',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Keep'}]}]}
 const target=globalThis.document.createElement('div'),onchange=vi.fn();const component=mount(Captions,{target,props:{composition,playheadTicks:1000000,onchange}});await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await tick()
 expect(onchange).not.toHaveBeenCalled();expect(target.querySelector('[role=alert]')?.textContent).toContain('уже есть субтитр')
 await unmount(component)
})

it('prunes a selected split part when Undo removes it, allowing one-cue editing again',async()=>{
 const {default:History}=await import('./test-fixtures/CaptionHistoryHarness.svelte')
 const before:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',clips:[{id:'left',timelineStartTick:0,timelineEndTick:2000000,text:'Hello'}]}]}
 const after:Composition={...before,captionTracks:[{...before.captionTracks![0],clips:[{...before.captionTracks![0].clips[0],timelineEndTick:1000000},{id:'right',timelineStartTick:1000000,timelineEndTick:2000000,text:'Hello'}]}]}
 const target=globalThis.document.createElement('div'),component=mount(History,{target,props:{before,after}});await tick()
 const select=(id:string)=>{const checkbox=target.querySelector<HTMLInputElement>(`input[aria-label="Выбрать субтитр ${id}"]`)!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}))}
 select('right');await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await tick()
 expect(target.querySelector('input[aria-label="Выбрать субтитр right"]')).toBeNull()
 select('left');await tick()
 expect(target.querySelector('input[aria-label="Спикер выбранного субтитра"]')).not.toBeNull()
 const merge=[...target.querySelectorAll('button')].find(button=>button.textContent?.includes('Объединить'))!
 expect(merge.disabled).toBe(true)
 await unmount(component)
})

it('seeks and selects captions on locked tracks without an authored-document commit',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',locked:true,clips:[{id:'first',timelineStartTick:0,timelineEndTick:1000000,text:'First'},{id:'second',timelineStartTick:2000000,timelineEndTick:3000000,text:'Second'}]}]}
 const target=globalThis.document.createElement('div'),onchange=vi.fn(),onseek=vi.fn()
 const component=mount(Captions,{target,props:{composition,playheadTicks:0,onchange,onseek}});await tick()
 const next=[...target.querySelectorAll('button')].find(button=>button.textContent==='Следующий субтитр')!
 next.click();await tick();expect(onseek).toHaveBeenLastCalledWith(2000000,false)
 expect(target.querySelector<HTMLInputElement>('input[aria-label="Выбрать субтитр second"]')!.checked).toBe(true)
 target.querySelector<HTMLButtonElement>('button[aria-label="Перейти к субтитру first"]')!.click();await tick()
 expect(onseek).toHaveBeenLastCalledWith(0,true)
 expect(target.querySelector<HTMLInputElement>('input[aria-label="Выбрать субтитр first"]')!.checked).toBe(true)
 expect(target.querySelector<HTMLTextAreaElement>('textarea[aria-label="Текст субтитра first"]')!.disabled).toBe(true)
 expect(onchange).not.toHaveBeenCalled()
 await unmount(component)
})

it('adds a caption to the explicitly active track while preserving its neighbour',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'first',name:'First',clips:[]},{id:'second',name:'Second',clips:[]}]}
 vi.mocked(editCompositionCaptions).mockResolvedValue({composition:buildCompositionWireDocument({...composition,captionTracks:[composition.captionTracks![0],{...composition.captionTracks![1],clips:[{id:'added',timelineStartTick:0,timelineEndTick:3000000,text:'Субтитр'}]}]}),appliedDelta:null,deleted:0})
 const target=globalThis.document.createElement('div'),onchange=vi.fn()
 const component=mount(Captions,{target,props:{composition,onchange}});await tick()
 const selector=target.querySelector<HTMLSelectElement>('[aria-label="Активная дорожка субтитров"]')!;selector.value='second';selector.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 const next=onchange.mock.calls[0][0] as Composition
 expect(next.captionTracks![0]).toEqual(composition.captionTracks![0]);expect(next.captionTracks![1].clips).toHaveLength(1)
 expect(composition.captionTracks![1].clips).toHaveLength(0)
 await unmount(component)
})

it('exports explicitly active track without an authored commit',async()=>{
 vi.mocked(writeCaptionFile).mockResolvedValue({extension:'srt',dataBase64:btoa('exported')})
 const create=vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:active-export'),revoke=vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'first',name:'First',clips:[]},{id:'second',name:'Second',clips:[{id:'second-cue',timelineStartTick:0,timelineEndTick:1000000,text:'Selected'}]}]}
 const target=globalThis.document.createElement('div'),onchange=vi.fn(),component=mount(Captions,{target,props:{composition,onchange}});await tick()
 const selector=target.querySelector<HTMLSelectElement>('[aria-label="Активная дорожка субтитров"]')!;selector.value='second';selector.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const button=[...target.querySelectorAll('button')].find(button=>button.textContent==='Экспортировать дорожку субтитров')!;button.click()
 await vi.waitFor(()=>expect(target.querySelector('a[download]')?.getAttribute('download')).toBe('Second.srt'))
 expect(vi.mocked(writeCaptionFile).mock.calls.at(-1)![0].cues[0].text).toBe('Selected');expect(onchange).not.toHaveBeenCalled()
 await unmount(component);expect(revoke).toHaveBeenCalledWith('blob:active-export');create.mockRestore();revoke.mockRestore()
})

it('advances past a fractional caption after projecting its exact navigation anchor',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:24000/1001,fpsRate:{numerator:24000,denominator:1001},backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Exact',clips:[{id:'first',timelineStartTick:'137729592000',timelineEndTick:'148324176000',text:'First'},{id:'next',timelineStartTick:'148324176000',timelineEndTick:'158918760000',text:'Next'}]}]}
 const target=globalThis.document.createElement('div'),onseek=vi.fn(),onchange=vi.fn(),component=mount(Captions,{target,props:{composition,playheadTicks:542208,onseek,onchange}});await tick()
 const next=[...target.querySelectorAll('button')].find(button=>button.textContent==='Следующий субтитр')!;next.click();await tick();expect(onseek).toHaveBeenLastCalledWith(542208,false)
 next.click();await tick();expect(onseek).toHaveBeenLastCalledWith(583916,false)
 expect(target.querySelector<HTMLInputElement>('[aria-label="Выбрать субтитр next"]')!.checked).toBe(true);expect(onchange).not.toHaveBeenCalled()
 await unmount(component)
})

it('creates the first track and commits an exact native Add response once',async()=>{
 const composition=createComposition({width:320,height:180,fps:24000/1001,fpsRate:{numerator:24000,denominator:1001},backgroundColor:'#000000'})
 const cue={id:'native-added',timelineStartTick:'137729592000',timelineEndTick:'900539640000',text:'Субтитр'}
 vi.mocked(editCompositionCaptions).mockImplementation(async wire=>({composition:{...wire,captionTracks:[{...wire.captionTracks![0],clips:[cue]}]},appliedDelta:null,deleted:0}))
 const target=globalThis.document.createElement('div'),onchange=vi.fn()
 const component=mount(Captions,{target,props:{composition,playheadTicks:542209,onchange}});await tick()
 target.querySelector<HTMLButtonElement>('button')!.click()
 await vi.waitFor(()=>expect(onchange).toHaveBeenCalledOnce())
 const [prepared,operation]=vi.mocked(editCompositionCaptions).mock.lastCall!
 expect(prepared.captionTracks![0]).toMatchObject({name:'Subtitle',format:'Subtitle',language:'en',syncLock:true,clips:[]})
 expect(operation).toEqual({kind:'add',trackId:prepared.captionTracks![0].id,startTick:542209,text:'Субтитр'})
 expect(onchange.mock.calls[0][0].captionTracks[0].clips[0]).toMatchObject(cue)
 expect(composition.captionTracks).toBeUndefined()
 await unmount(component)
})
it('ignores a pending native Add response after unmount',async()=>{
 const composition=createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'})
 let resolve!:(value:Awaited<ReturnType<typeof editCompositionCaptions>>)=>void
 vi.mocked(editCompositionCaptions).mockImplementation(()=>new Promise(done=>resolve=done))
 const target=globalThis.document.createElement('div'),onchange=vi.fn()
 const component=mount(Captions,{target,props:{composition,onchange}});await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await tick()
 const prepared=vi.mocked(editCompositionCaptions).mock.lastCall![0]
 await unmount(component)
 resolve({composition:prepared,appliedDelta:null,deleted:0});await tick()
 expect(onchange).not.toHaveBeenCalled()
})
