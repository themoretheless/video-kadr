// @ts-expect-error Mounted tests require Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedEmbeddedAudioInspector.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn(),getAudioDspCatalog:vi.fn(async()=>({schemaVersion:1,effects:[{id:'concat.echo',nativeExport:true,name:'Echo',params:[{id:'delay',name:'Delay',min:0.05,max:1,default:0.25,unit:'seconds',choices:[]}]}]}))}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{sources:{source:{hasAudio:true}},tracks:[{kind:'video',id:'v',name:'Video',locked:false,clips:[{id:'clip',sourceId:'source',sourceAudioEnabled:true,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},dspEffects:[{id:'concat.echo',params:{delay:0.25}}]}]}]},instances:[]}}} as unknown as NativeNestedProject;nestedComposition.renderedOutputs={root:'stale'};vi.clearAllMocks()})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})
it('edits embedded DSP on its video clip, invalidates cached output and restores undo/redo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();await vi.waitFor(()=>expect(document.querySelector('input[aria-label="DSP 1 Delay"]')).not.toBeNull())
 const input=document.querySelector<HTMLInputElement>('input[aria-label="DSP 1 Delay"]')!;input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const clip=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!};expect(clip().dspEffects?.[0]?.params?.delay).toBe(0.5);expect(clip().independentAudio).toBe(true);expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(clip().dspEffects?.[0]?.params?.delay).toBe(0.25);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});redoNestedComposition();expect(clip().dspEffects?.[0]?.params?.delay).toBe(0.5)
})

it('edits an embedded fade through native history without changing picture opacity',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Нарастание звука"]')!;expect(input).not.toBeNull()
 input.value='2';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const clip=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!}
 expect(clip().audioFadeInTicks).toBe(2000000);expect(clip().independentAudio).toBe(true);expect(clip().opacity).toBeUndefined();expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(clip().audioFadeInTicks).toBeUndefined();redoNestedComposition();expect(clip().audioFadeInTicks).toBe(2000000)
})

it('adds retained source keys through native history while static DSP remains unchanged',async()=>{
 component=mount(Inspector,{target:document.body});await tick();await vi.waitFor(()=>expect(document.querySelector('[aria-label="DSP 1 Delay: добавить исходный ключ"]')).not.toBeNull())
 document.querySelector<HTMLButtonElement>('[aria-label="DSP 1 Delay: добавить исходный ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const effect=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.dspEffects![0]!}
 expect(effect().concatSourceKeys).toMatchObject({delay:[{at:0.5,value:0.25}]});expect(effect().params).toEqual({delay:0.25});expect(effect().animation).toBeUndefined();undoNestedComposition();expect(effect().concatSourceKeys).toBeUndefined();redoNestedComposition();expect(effect().concatSourceKeys?.delay).toHaveLength(1)
})
