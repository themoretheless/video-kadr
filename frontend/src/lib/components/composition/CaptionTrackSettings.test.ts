// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Settings from './CaptionTrackSettings.svelte'
import {createComposition} from '$lib/composition/commands'
import type {Composition} from '$lib/composition/types'
it('creates a front caption track with source format defaults and activates it in one commit',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'existing',name:'Keep',clips:[]}]}
 const target=document.createElement('div'),onchange=vi.fn(),onactive=vi.fn()
 const component=mount(Settings,{target,props:{composition,onchange,onactive}});await tick()
 const format=target.querySelector<HTMLSelectElement>('[aria-label="Формат новой дорожки субтитров"]')!;format.value='CEA-608';format.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 target.querySelector<HTMLButtonElement>('button')!.click();await tick()
 expect(onchange).toHaveBeenCalledOnce()
 const next=onchange.mock.calls[0][0] as Composition
 expect(next.captionTracks![0]).toMatchObject({name:'CEA-608',format:'CEA-608',language:'en',syncLock:true,hidden:false,locked:false,style:{backgroundColor:'#000000ff'},clips:[]})
 expect(next.captionTracks![1]).toBe(composition.captionTracks![0]);expect(onactive).toHaveBeenCalledWith(next.captionTracks![0].id)
 expect(composition.captionTracks).toHaveLength(1)
 await unmount(component)
})
it('changes track language without losing file metadata or cue identifiers',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'track',name:'Captions',vttBlocks:['NOTE retained'],clips:[{id:'cue',cueId:'source-id',timelineStartTick:0,timelineEndTick:1000000,text:'Keep'}]}]}
 const target=document.createElement('div'),onchange=vi.fn()
 const component=mount(Settings,{target,props:{composition,onchange,onactive:vi.fn()}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Язык дорожки субтитров"]')!;input.value='fr';input.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(onchange.mock.calls[0][0].captionTracks[0]).toEqual({...composition.captionTracks![0],language:'fr'})
 expect(composition.captionTracks![0].language).toBeUndefined()
 await unmount(component)
})
it('keeps unlock available while locked metadata and deletion controls are disabled',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'track',name:'Captions',locked:true,clips:[]}]}
 const target=document.createElement('div'),onchange=vi.fn()
 const component=mount(Settings,{target,props:{composition,onchange,onactive:vi.fn()}});await tick()
 expect(target.querySelector('[aria-label="Название дорожки субтитров"]')!.closest('fieldset')!.disabled).toBe(true)
 const lock=target.querySelector<HTMLInputElement>('[aria-label="Блокировка дорожки субтитров"]')!;expect(lock.matches(':disabled')).toBe(false)
 lock.checked=false;lock.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(onchange.mock.calls[0][0].captionTracks[0]).toEqual({...composition.captionTracks![0],locked:false})
 await unmount(component)
})

it('deletes only the active unlocked track and activates the surviving neighbour',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'first',name:'Keep',clips:[]},{id:'second',name:'Delete',clips:[{id:'cue',timelineStartTick:0,timelineEndTick:1000000,text:'Delete with track'}]}]}
 const target=document.createElement('div'),onchange=vi.fn(),onactive=vi.fn()
 const component=mount(Settings,{target,props:{composition,activeId:'second',onchange,onactive}});await tick()
 const remove=[...target.querySelectorAll('button')].find(button=>button.textContent==='Удалить дорожку субтитров')!;remove.click();await tick()
 expect(onchange).toHaveBeenCalledOnce();expect(onchange.mock.calls[0][0].captionTracks).toEqual([composition.captionTracks![0]])
 expect(onactive).toHaveBeenCalledWith('first');expect(composition.captionTracks).toHaveLength(2)
 await unmount(component)
})


it('shows and hides all tracks including locked tracks in a single authored change',async()=>{
 const composition:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'first',name:'Visible',clips:[]},{id:'second',name:'Locked hidden',locked:true,hidden:true,language:'fr',vttBlocks:['NOTE keep'],clips:[]}]}
 const target=document.createElement('div'),onchange=vi.fn(),onactive=vi.fn()
 const component=mount(Settings,{target,props:{composition,activeId:'second',onchange,onactive}});await tick()
 for(const [label,hidden] of [['Показать все субтитры',false],['Скрыть все субтитры',true]] as const){
  const button=[...target.querySelectorAll('button')].find(button=>button.textContent===label)!;expect(button.matches(':disabled')).toBe(false);button.click();await tick()
  expect(onchange.mock.calls.at(-1)![0].captionTracks).toEqual(composition.captionTracks!.map(track=>({...track,hidden})))
 }
 expect(onchange).toHaveBeenCalledTimes(2);expect(onactive).not.toHaveBeenCalled()
 expect(composition.captionTracks![0].hidden).toBeUndefined()
 await unmount(component)
})
