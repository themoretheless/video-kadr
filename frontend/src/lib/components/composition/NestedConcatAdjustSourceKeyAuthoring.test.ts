import {writeFileSync} from 'node:fs'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatColorWheelsInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.timeUs=600000
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
it('authors all original axes with incoming custom easing and persists exact UI output',async()=>{
 component=mount(Inspector,{target:document.body,props:{kind:'concat_adjust'}});await tick();click('Добавить Concat Adjust');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const field=async(name:string,value:string)=>{const el=document.querySelector<HTMLInputElement>(`input[aria-label="${name}"]`)!;expect(el).not.toBeNull();el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
 let count=1
 const add=async(channel:string)=>{document.querySelector<HTMLButtonElement>(`button[aria-label="clip Concat colorWheels ${channel} добавить"]`)!.click();count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 for(const[channel,end]of Object.entries({'lift.x':0.2,'lift.y':-0.6,'lift.m':0.1,'gamma.x':-0.1,'gamma.y':0.4,'gamma.m':-0.25,'gain.x':0.35,'gain.y':0.45,'gain.m':0.1})){
  const prefix=`clip Concat colorWheels ${channel}`
  await field(`${prefix} new at`,'0');await field(`${prefix} new value`,'0');await add(channel)
  await field(`${prefix} new at`,'1');await field(`${prefix} new value`,String(end));await add(channel)
  for(const[axis,value]of [[0,'0.42'],[1,'-0.5'],[2,'0.58']]as const){await field(`${prefix} 1 ease ${axis}`,value);count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 }
 const effects=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.effects}
 const saved=JSON.parse(JSON.stringify(effects()));expect(Object.keys(saved[0].concatKeys)).toHaveLength(9);expect(saved[0].concatKeys['gain.m'][1]).toEqual({at:1,value:0.1,ease:[0.42,-0.5,0.58,1]})
 if(process.env.CONCAT_ADJUST_UI_OUTPUT)writeFileSync(process.env.CONCAT_ADJUST_UI_OUTPUT,JSON.stringify(saved[0]))
 undoNestedComposition();expect(JSON.stringify(effects())).not.toBe(JSON.stringify(saved));redoNestedComposition();expect(JSON.parse(JSON.stringify(effects()))).toEqual(saved)
})
