// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatWhiteBalanceInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}}as unknown as NativeNestedProject})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name)!.click()}
function effect(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video');const value=track.clips[0]!.effects[0]!;if(value.kind!=='concat_white_balance')throw new Error('vibrance');return value}
async function input(channel:string,value:string,count:number){const node=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat White Balance ${channel}"]`)!;node.value=value;node.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('authors temperature and filter tint with independent local curves, reset and Undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat White Balance');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const temperature=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat White Balance temperature"]')!;expect([temperature.min,temperature.max,temperature.step,temperature.value]).toEqual(['2000','11000','50','6500'])
 const strength=document.querySelector<HTMLInputElement>('input[aria-label="clip Concat White Balance tint"]')!;expect(strength.value).toBe('0')
 await input('temperature','4500',2);nestedComposition.timeUs=600000;await tick();document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat White Balance temperature ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 await input('tint','25',4);document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat White Balance tint ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 expect(effect().animation?.temperature).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:4500}]}})
 expect(effect().animation?.tint).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:25}]}})
 click('Сбросить Concat White Balance');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));expect(effect().parameters).toEqual({});undoNestedComposition();expect(effect().parameters).toEqual({temperature:4500,tint:25});expect(effect().animation?.temperature).toBeDefined();expect(effect().animation?.tint).toBeDefined()
})
