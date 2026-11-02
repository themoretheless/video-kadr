// @ts-expect-error Tests need Svelte client mount under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,expect,it} from 'vitest'
import {nestedComposition,nestedHistory,nestedPlayback,closeNestedComposition} from '$lib/state/nestedComposition.svelte'
import NestedSourceKeyMarks from './NestedSourceKeyMarks.svelte'
afterEach(()=>{closeNestedComposition();document.body.innerHTML=''})
it('seeks from an accessible source diamond and pauses playback without document history',async()=>{
 closeNestedComposition();nestedPlayback.playing=true;nestedComposition.timeUs=0
 const component=mount(NestedSourceKeyMarks,{target:document.body,props:{marks:[{timeUs:1500000,count:1},{timeUs:2500000,count:3}]}})
 try{await tick();const button=document.querySelector<HTMLButtonElement>('[aria-label="Исходный ключ 2500000 мкс (3)"]')!;button.click();expect(nestedComposition.timeUs).toBe(2500000);expect(nestedPlayback.playing).toBe(false);expect(nestedHistory.past).toHaveLength(0);expect(button.title).toContain('ключей: 3')}finally{await unmount(component)}
})
it('disables navigation under the parent transaction lock',async()=>{
 const component=mount(NestedSourceKeyMarks,{target:document.body,props:{marks:[{timeUs:1500000,count:1}],disabled:true}})
 try{await tick();const button=document.querySelector('button')!;expect(button.disabled).toBe(true);button.click();expect(nestedComposition.timeUs).toBe(0)}finally{await unmount(component)}
})
it('cancels a drag and refuses release after the selected project changes',async()=>{
 closeNestedComposition();const original={schemaVersion:1 as const,root:'root',compositions:{}};nestedComposition.document=original
 const component=mount(NestedSourceKeyMarks,{target:document.body,props:{marks:[{timeUs:500000,count:1}],compositionId:'root',clipIds:['shape'],rangeStart:0,rangeEnd:1000000}})
 try{
  await tick();const button=document.querySelector('button')!,ruler=button.parentElement!;Object.defineProperty(button,'setPointerCapture',{value:()=>{}});Object.defineProperty(ruler,'getBoundingClientRect',{value:()=>({width:100})})
  const pointer=(type:string,x:number)=>{const event=new Event(type,{bubbles:true,cancelable:true});Object.defineProperties(event,{button:{value:0},pointerId:{value:1},clientX:{value:x}});button.dispatchEvent(event)}
  pointer('pointerdown',50);pointer('pointermove',60);await tick();expect(document.querySelector('[role=status]')?.textContent).toContain('0.600');pointer('pointercancel',60);pointer('pointerup',60);await tick();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(original)
  pointer('pointerdown',50);pointer('pointermove',60);nestedComposition.document={...original,root:'replacement'};pointer('pointerup',60);await tick();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document.root).toBe('replacement')
 }finally{await unmount(component)}
})
