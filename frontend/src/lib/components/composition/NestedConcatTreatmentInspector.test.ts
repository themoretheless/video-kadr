// @ts-expect-error Client entry required for mounted Svelte components.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {upgradeConcatEffectLinks,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,editNestedVideoEffects,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatTreatmentInspector.svelte'
import Stack from './NestedEffectStack.svelte'
import Exposure from './NestedConcatExposureInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn(),upgradeConcatEffectLinks:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
function project(locked=false):NativeNestedProject{
 const constant=(value:number)=>({mode:'constant' as const,value})
 return {schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}},tracks:[{kind:'graphic',id:'shapes',name:'Shapes',hidden:false,locked,clips:[{id:'shape',timelineStartTick:0,timelineEndTick:2000000,width:160,height:120,graphic:{layers:[]},concatTreatment:{strength:0.5,fadeIn:0.5,fadeOut:0.5,durationTicks:2000000},effects:[],transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),enabled:true}]}]}}}}
}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project();nestedComposition.renderedOutputs={root:'cached'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;agentSession.lock=null;closeNestedComposition();document.body.innerHTML=''})
async function start(){component=mount(Inspector,{target:document.body});await tick()}
function change(field:string,value:string){const input=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="Concat treatment ${field}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))}
function shape(){const track=nestedComposition.document!.compositions.root.content.tracks[0];if(track.kind!=='graphic')throw new Error('graphic required');return track.clips[0]}
it('changes strength once and preserves timing/ramps through Undo/Redo',async()=>{
 await start();const original=JSON.parse(JSON.stringify(nestedComposition.document));change('strength','0.8');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(shape().concatTreatment).toEqual({strength:0.8,fadeIn:0.5,fadeOut:0.5,durationTicks:2000000});expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(nestedComposition.document).toEqual(original);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'});redoNestedComposition();expect(shape().concatTreatment?.strength).toBe(0.8)
})
it('edits a ramp and refuses native validation failures without changing the document',async()=>{
 await start();change('fadeIn','1.5');await vi.waitFor(()=>expect(shape().concatTreatment?.fadeIn).toBe(1.5));const count=nestedHistory.past.length;vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('invalid strength'));change('strength','2');await vi.waitFor(()=>expect(nestedHistory.error).toContain('invalid strength'));expect(shape().concatTreatment?.strength).toBe(0.5);expect(nestedHistory.past).toHaveLength(count)
})
it('disables locked tracks and refuses locks acquired during native validation',async()=>{
 nestedComposition.document=project(true);await start();expect(document.querySelector('fieldset')!.disabled).toBe(true);nestedComposition.document=project();await tick();let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));change('fadeOut','1.5');await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish();await vi.waitFor(()=>expect(nestedHistory.error).toContain('Held'));expect(shape().concatTreatment?.fadeOut).toBe(0.5);expect(nestedHistory.past).toHaveLength(0)
})

it('keeps the selector usable when the current track is locked and edits the chosen unlocked layer',async()=>{
 const input=project(true),node=input.compositions.root,first=node.content.tracks[0];if(first.kind!=='graphic')throw new Error('graphic required')
 node.content={...node.content,tracks:[first,{...first,id:'other',locked:false,clips:first.clips.map(clip=>({...clip,id:'other-layer'}))}]};nestedComposition.document=input;await start()
 const select=document.querySelector<HTMLSelectElement>('[aria-label="Корректирующий клип Concat"]')!;expect(select.disabled).toBe(false);expect(document.querySelector('fieldset')!.disabled).toBe(true);select.value='other-layer';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(document.querySelector('fieldset')!.disabled).toBe(false)
 change('strength','0.9');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(shape().concatTreatment?.strength).toBe(0.5);expect(nestedComposition.document!.compositions.root.content.tracks[1].clips[0]).toMatchObject({concatTreatment:{strength:0.9}})
})

it('mounts treatment stack controls and commits reorder, bypass and removal through history',async()=>{
 await editNestedVideoEffects('root','shapes','shape',()=>[{kind:'concat_exposure',parameters:{stops:1}},{kind:'concat_contrast',parameters:{amount:20}}]);component=mount(Stack,{target:document.body});await tick()
 expect(document.querySelector('[aria-label="shape effect 0 down"]')).not.toBeNull();expect(document.querySelector('[aria-label="shape initialize Concat animation"]')).toBeNull()
 document.querySelector<HTMLButtonElement>('[aria-label="shape effect 0 down"]')!.click();await vi.waitFor(()=>expect(shape().effects[0].kind).toBe('concat_contrast'));undoNestedComposition();await tick();expect(shape().effects[0].kind).toBe('concat_exposure')
 document.querySelector<HTMLButtonElement>('[aria-label="shape effect 0 enable"]')!.click();await vi.waitFor(()=>expect(shape().effects[0].kind).toBe('native_bypass'));await tick()
 document.querySelector<HTMLButtonElement>('[aria-label="shape effect 1 remove"]')!.click();await vi.waitFor(()=>expect(shape().effects).toHaveLength(1));undoNestedComposition();expect(shape().effects).toHaveLength(2);expect(shape().concatTreatment).toEqual({strength:0.5,fadeIn:0.5,fadeOut:0.5,durationTicks:2000000});expect(nestedHistory.error).toBe('')
})

it('appends Rust-resolved defaults from the mounted inspector as one undoable edit',async()=>{
 vi.mocked(upgradeConcatEffectLinks).mockResolvedValue({schemaVersion:1,links:[{upgraded:false,link:{id:'concat.mono'}}],nativeEffects:[{kind:'concat_exposure',parameters:{stops:1}}]});await start();const before=JSON.parse(JSON.stringify(nestedComposition.document));[...document.querySelectorAll('button')].find(button=>button.textContent==='Добавить эффект в корректирующий слой')!.click();await vi.waitFor(()=>expect(shape().effects).toHaveLength(1));expect(upgradeConcatEffectLinks).toHaveBeenCalledWith([{id:'concat.mono',enabled:true}],undefined,true);expect(shape().concatTreatment).toEqual({strength:0.5,fadeIn:0.5,fadeOut:0.5,durationTicks:2000000});expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).toEqual(before);redoNestedComposition();expect(shape().effects).toHaveLength(1)
})

it('authors treatment exposure with native keys on the absolute timeline clock',async()=>{
 const input=project(),track=input.compositions.root.content.tracks[0];if(track.kind!=='graphic')throw new Error('graphic required');input.compositions.root.content={...input.compositions.root.content,tracks:[{...track,clips:track.clips.map(clip=>({...clip,timelineStartTick:1000000,timelineEndTick:3000000,effects:[{kind:'concat_exposure',parameters:{stops:1}}]}))}]};nestedComposition.document=input;nestedComposition.timeUs=1500000;component=mount(Exposure,{target:document.body});await tick()
 const control=document.querySelector<HTMLInputElement>('[aria-label="shape Concat Exposure stops"]')!;control.value='2';control.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();document.querySelector<HTMLButtonElement>('[aria-label="shape Concat Exposure stops ключ"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(shape().effects[0]).toMatchObject({parameters:{stops:2},animation:{mode:'keyframes',track:{keyframes:[{tick:1500000,value:2}]}}});undoNestedComposition();expect(shape().effects[0]).not.toHaveProperty('animation')
})
