// @ts-expect-error Client entry required for mounted Svelte components.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatShapeInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
function project(locked=false):NativeNestedProject{
 const constant=(value:number)=>({mode:'constant' as const,value})
 return {schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}},tracks:[{kind:'graphic',id:'shapes',name:'Shapes',hidden:false,locked,clips:[{id:'shape',timelineStartTick:0,timelineEndTick:2000000,width:160,height:120,graphic:{layers:[]},concatShape:{kind:'arrow',size:0.5,fill:'#ff804080',stroke:'#102030ff',strokeWidth:0.02},concatShapeBlend:'screen',effects:[],transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),enabled:true}]}]}}}}
}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project();nestedComposition.renderedOutputs={root:'cached'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;agentSession.lock=null;closeNestedComposition();document.body.innerHTML=''})
async function start(){component=mount(Inspector,{target:document.body});await tick()}
function change(field:string,value:string){const input=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="Concat shape ${field}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))}
function shape(){const track=nestedComposition.document!.compositions.root.content.tracks[0];if(track.kind!=='graphic')throw new Error('graphic required');return track.clips[0]}
it('exposes seven figures and six blends and preserves other style fields through history',async()=>{
 await start();expect(document.querySelectorAll('[aria-label="Concat shape kind"] option')).toHaveLength(7);expect(document.querySelectorAll('[aria-label="Concat shape blend"] option')).toHaveLength(6)
 const original=JSON.parse(JSON.stringify(nestedComposition.document));change('kind','circle');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(shape().concatShape).toEqual({...original.compositions.root.content.tracks[0].clips[0].concatShape,kind:'circle'});expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(original);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'});redoNestedComposition();expect(shape().concatShape?.kind).toBe('circle')
 await tick();change('blend','multiply');await vi.waitFor(()=>expect(shape().concatShapeBlend).toBe('multiply'));expect(shape().concatShape?.kind).toBe('circle')
})
it('retains empty fill for outline-only figures and rejects invalid native candidates atomically',async()=>{
 await start();change('fill','');await vi.waitFor(()=>expect(shape().concatShape?.fill).toBe(''));const count=nestedHistory.past.length;vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('invalid Concat shape size'));change('size','-1');await vi.waitFor(()=>expect(nestedHistory.error).toContain('invalid'));expect(shape().concatShape?.size).toBe(0.5);expect(nestedHistory.past).toHaveLength(count)
})
it('disables locked tracks and refuses a late agent lock',async()=>{
 nestedComposition.document=project(true);await start();expect(document.querySelector('fieldset')!.disabled).toBe(true);nestedComposition.document=project();await tick();let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));change('size','0.7');await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish();await vi.waitFor(()=>expect(nestedHistory.error).toContain('Held'));expect(shape().concatShape?.size).toBe(0.5);expect(nestedHistory.past).toHaveLength(0)
})
