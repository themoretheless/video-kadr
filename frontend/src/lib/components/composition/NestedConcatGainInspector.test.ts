import {sourceRunClipboard} from '$lib/state/sourceRunClipboard.svelte'
import {captureSourceRunClipboard} from '$lib/composition/sourceRunClipboard'
// @ts-expect-error Mounted tests require Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatGainInspector.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.clearAllMocks()
 const placement={timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1}
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'audio',id:'a',name:'Audio',locked:false,clips:[{id:'sound',placement,gain:{mode:'constant',value:0.9},concatGainKeys:[{at:0,value:0.25},{at:1,value:1}]}]},{kind:'video',id:'v',name:'Video',locked:false,clips:[{id:'picture',placement,audioGain:{mode:'constant',value:0.9}}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.timeUs=500000;nestedComposition.renderedOutputs={root:'stale'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})
function clip(index=0){const track=nestedComposition.document!.compositions.root!.content.tracks[index]!;if(track.kind!=='audio'&&track.kind!=='video')throw new Error('track');return track.clips[0]!}
it('commits a source value with validation, output invalidation and undo/redo',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Ключ громкости 2: значение"]')!;input.value='1.5';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(clip()).toMatchObject({concatGainKeys:[{value:0.25},{value:1.5,derived:false}],gain:{mode:'constant',value:0.9}})
 expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![0].compositions.root!.content.tracks[0]).toMatchObject({clips:[{concatGainKeys:[{value:0.25},{value:1.5}]}]})
 expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(clip().concatGainKeys?.[1]!.value).toBe(1);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});redoNestedComposition();expect(clip().concatGainKeys?.[1]!.value).toBe(1.5)
})
it('adds at the cursor on a video and keeps dry independent mixing after removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();const select=document.querySelector<HTMLSelectElement>('select[aria-label="Клип для ключей громкости"]')!;select.value='picture';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const add=Array.from(document.querySelectorAll('button')).find(button=>button.textContent?.includes('Добавить ключ громкости'))!;add.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(clip(1)).toMatchObject({independentAudio:true,concatGainKeys:[{at:0.5,value:0.9}]})
 await tick();document.querySelector<HTMLButtonElement>('button[aria-label="Удалить ключ громкости 1"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(clip(1)).toMatchObject({independentAudio:true,concatGainKeys:[]})
})
it('does not replace the document when clip selection changes during validation',async()=>{
 let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
 component=mount(Inspector,{target:document.body});await tick();const before=nestedComposition.document
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Ключ громкости 2: значение"]')!;input.value='2';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(finish).toBeDefined())
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Клип для ключей громкости"]')!;select.value='picture';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false))
 expect(nestedComposition.document).toBe(before);expect(nestedHistory.past).toHaveLength(0)
})

it('publishes a native gain cut after the editor unmounts while validation is pending',async()=>{
 const old=captureSourceRunClipboard([{at:0,value:7}],[0]);sourceRunClipboard.value=old;let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
 component=mount(Inspector,{target:document.body});await tick();const before=JSON.stringify(nestedComposition.document);document.querySelector('[aria-label="Громкость Concat исходный ключ 0"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'a',ctrlKey:true,bubbles:true}));await tick()
 ;[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Вырезать исходные ключи')!.click();await vi.waitFor(()=>expect(finish).toBeDefined());await unmount(component!);component=undefined;expect(sourceRunClipboard.value).toBe(old);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(clip().concatGainKeys).toEqual([]);expect(sourceRunClipboard.value?.keys).toEqual([{at:0,value:.25},{at:1,value:1}]);undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
