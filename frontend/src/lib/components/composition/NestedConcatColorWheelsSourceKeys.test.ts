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
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[{kind:'concat_color_wheels',parameters:{},concatKeys:{'lift.x':[{at:0,value:0},{at:1,value:0.5}]}}]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.timeUs=600000
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
it('guards original source keys against conflicting wheel edits and clearing restores controls',async()=>{

 component=mount(Inspector,{target:document.body});await tick()
 const button=()=>document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Color Wheels lift общий ключ"]')!
 expect(button().matches(':disabled')).toBe(true)
 click('Удалить ключи Concat');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(button().matches(':disabled')).toBe(false)
 undoNestedComposition();await tick();expect(button().matches(':disabled')).toBe(true)
 redoNestedComposition();await tick();expect(button().matches(':disabled')).toBe(false)
})
