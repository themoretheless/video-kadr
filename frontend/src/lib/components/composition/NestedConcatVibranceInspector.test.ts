// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatVibranceInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}}as unknown as NativeNestedProject})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name)!.click()}
function effect(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video');const value=track.clips[0]!.effects[0]!;if(value.kind!=='concat_vibrance')throw new Error('vibrance');return value}
async function input(channel:string,value:string,count:number){const node=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Vibrance ${channel}"]`)!;node.value=value;node.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('authors amount and filter intensity with independent local curves, reset and Undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Vibrance');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const amount=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Vibrance amount"]')!;expect([amount.min,amount.max,amount.step,amount.value]).toEqual(['0.1','2','0.05','0.7'])
 const strength=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Vibrance intensity"]')!;expect(strength.value).toBe('100')
 await input('amount','1.25',2);nestedComposition.timeUs=600000;await tick();document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Vibrance amount ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 await input('intensity','25',4);document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Vibrance intensity ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 expect(effect().animation?.amount).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:1.25}]}})
 expect(effect().animation?.intensity).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:25}]}})
 click('Сбросить Concat Vibrance');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));expect(effect().parameters).toEqual({});undoNestedComposition();expect(effect().parameters).toEqual({amount:1.25,intensity:25});expect(effect().animation?.amount).toBeDefined();expect(effect().animation?.intensity).toBeDefined()
})
