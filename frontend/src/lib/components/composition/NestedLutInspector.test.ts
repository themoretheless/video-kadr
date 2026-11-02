// @ts-expect-error Svelte client entry for DOM tests.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {validateColorLut,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedLutInspector.svelte'
vi.mock('$lib/api',()=>({validateColorLut:vi.fn(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});vi.mocked(validateColorLut).mockResolvedValue({title:'Swap',shaperSize:2,cubeSize:2});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject;nestedComposition.renderedOutputs={root:'movie'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function choose(name:string,text:string){const file=new File([text],name);Object.defineProperty(file,'text',{value:async()=>text});const input=document.querySelector<HTMLInputElement>('input[type=file]')!;Object.defineProperty(input,'files',{configurable:true,value:[file]});input.dispatchEvent(new Event('change',{bubbles:true}))}
function lut(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='color_lut')throw new Error('lut');return e.lut}
it('validates imported tables before history, edits intensity and restores output bindings on Undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();choose('grade.cube','LUT_1D_SIZE 2\n0 0 0\n1 1 1')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(lut().format).toBe('cube');expect(lut().intensity).toBe(100);expect(validateColorLut).toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({})
 const intensity=document.querySelector<HTMLInputElement>('input[type=number]')!;intensity.value='50';intensity.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(lut().intensity).toBe(50)
 undoNestedComposition();undoNestedComposition();expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
})
it('imports 3dl, rejects invalid files and aborts pending validation without committing after disposal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();choose('grade.3dl','0 1023\n0 0 0');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(lut().format).toBe('three_dl')
 vi.mocked(validateColorLut).mockRejectedValueOnce(new Error('invalid table'));choose('bad.cube','bad');await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toBe('invalid table'));expect(nestedHistory.past).toHaveLength(1)
 let resolve!:(value:{title:string;shaperSize:number;cubeSize:number})=>void;vi.mocked(validateColorLut).mockReturnValueOnce(new Promise(done=>{resolve=done}));choose('late.cube','late');await vi.waitFor(()=>expect(validateColorLut).toHaveBeenCalledTimes(3));const signal=vi.mocked(validateColorLut).mock.calls[2]![1]!
 await unmount(component);component=undefined;expect(signal.aborted).toBe(true);resolve({title:'Late',shaperSize:0,cubeSize:2});await tick();expect(nestedHistory.past).toHaveLength(1)
})
