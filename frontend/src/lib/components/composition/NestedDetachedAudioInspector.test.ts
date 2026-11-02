// @ts-expect-error Mounted tests require Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedDetachedAudioInspector.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.clearAllMocks()
 const placement={timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1}
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'v',name:'Video',locked:false,clips:[{id:'picture',placement,sourceAudioEnabled:false,audioGain:{mode:'constant',value:0.7}}],transitions:[]},{kind:'audio',id:'a',name:'Audio',locked:false,clips:[{id:'sound',placement,gain:{mode:'constant',value:0.2},detachedFrom:'picture',concatDetachedOrder:0,dspEffects:[{id:'concat.echo',params:{delay:0.25}}]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'stale'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})
it('reattaches through validation and restores the detached clips/outputs on undo',async()=>{
 const before=JSON.stringify(nestedComposition.document);component=mount(Inspector,{target:document.body});await tick()
 document.querySelector<HTMLButtonElement>('button')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const tracks=nestedComposition.document!.compositions.root!.content.tracks
 expect(tracks[0]).toMatchObject({clips:[{sourceAudioEnabled:true,audioGain:{value:0.7},dspEffects:[{id:'concat.echo'}]}]});expect(tracks[1]!.clips).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({});expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});redoNestedComposition();expect(nestedComposition.document!.compositions.root!.content.tracks[1]!.clips).toHaveLength(0)
})
