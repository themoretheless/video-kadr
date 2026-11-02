// @ts-expect-error Client entry required for mounted Svelte components.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatTitleInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
function project(locked=false):NativeNestedProject{return {schemaVersion:1,root:'root',compositions:{root:{content:{schemaVersion:1,timeBase:1000000,sources:{},canvas:{width:64,height:64,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}},tracks:[{id:'titles',kind:'text',name:'Titles',hidden:false,locked,clips:[{id:'title',text:'Hello',timelineStartTick:0,timelineEndTick:1000000,transform:{x:{mode:'constant',value:0},y:{mode:'constant',value:0},scaleX:{mode:'constant',value:1},scaleY:{mode:'constant',value:1},rotationDegrees:{mode:'constant',value:0},anchorX:0.5,anchorY:0.5},enabled:true,opacity:{mode:'constant',value:1},style:{fontFamily:'Noto Sans',fontSize:24,color:{red:1,green:1,blue:1,alpha:1},background:{red:0,green:0,blue:0,alpha:0},stroke:{red:0,green:0,blue:0,alpha:0},strokeWidth:0,shadow:{red:0,green:0,blue:0,alpha:0},shadowX:0,shadowY:0},concatTitle:{fontFamily:'Inter',tracking:0.01}}]}]},instances:[],durationUs:1000000}}} as NativeNestedProject}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project();nestedComposition.renderedOutputs={root:'old'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;agentSession.lock=null;closeNestedComposition();document.body.innerHTML=''})
async function start(){component=mount(Inspector,{target:document.body});await tick()}
function change(field:string,value:string){const input=document.querySelector<HTMLInputElement>(`[aria-label="Concat ${field}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))}
function style(){const track=nestedComposition.document!.compositions.root.content.tracks[0];return track.kind==='text'?track.clips[0].concatTitle:undefined}
it('mounts every source style control and commits validated edits with exact Undo/Redo',async()=>{
 await start();expect(document.querySelectorAll('[aria-label^="Concat "]')).toHaveLength(18)
 const original=JSON.parse(JSON.stringify(nestedComposition.document));change('fontSize','0.17')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(style()).toEqual({fontFamily:'Inter',tracking:0.01,fontSize:0.17});expect(evaluateNestedFrame).toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(original);expect(nestedComposition.renderedOutputs).toEqual({root:'old'});redoNestedComposition();expect(style()?.fontSize).toBe(0.17)
})
it('preserves the document and history when native validation rejects typography',async()=>{
 vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('invalid title size'));await start();change('fontSize','0.001');await vi.waitFor(()=>expect(nestedHistory.error).toContain('invalid title size'));expect(style()?.fontSize).toBeUndefined();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old'})
})
it('disables locked tracks and refuses a lock acquired during validation',async()=>{
 nestedComposition.document=project(true);await start();expect(document.querySelector<HTMLInputElement>('[aria-label="Concat fontSize"]')!.disabled||document.querySelector('fieldset')!.disabled).toBe(true)
 nestedComposition.document=project();await tick();let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));change('tracking','0.03');await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish();await vi.waitFor(()=>expect(nestedHistory.error).toContain('Held'));expect(style()?.tracking).toBe(0.01);expect(nestedHistory.past).toHaveLength(0)
})
it('refuses a title selection change while the native candidate is pending',async()=>{
 await start();let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));change('tracking','0.03');await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());nestedComposition.activeCompositionId='other';await tick();finish();await vi.waitFor(()=>expect(nestedHistory.error).toContain('изменился'));expect(style()?.tracking).toBe(0.01);expect(nestedHistory.past).toHaveLength(0)
})

it('saves multiline title drafts as one validated step and restores text and typography with Undo',async()=>{
 await start();const original=JSON.parse(JSON.stringify(nestedComposition.document))
 const input=document.querySelector<HTMLTextAreaElement>('[aria-label="Текст титра Concat"]')!;input.value='Новый титр\nSecond line';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const button=[...document.querySelectorAll('button')].find(button=>button.textContent==='Сохранить текст титра')!;button.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));const track=nestedComposition.document!.compositions.root.content.tracks[0];expect(track.clips[0]).toMatchObject({text:'Новый титр\nSecond line',concatTitle:{fontFamily:'Inter',tracking:0.01}});expect(nestedComposition.renderedOutputs).toEqual({})
 await tick();expect(document.querySelector('[role=alert]')).toBeNull();expect(button.disabled).toBe(true)
 undoNestedComposition();expect(nestedComposition.document).toEqual(original);redoNestedComposition();expect(nestedComposition.document!.compositions.root.content.tracks[0].clips[0]).toMatchObject({text:'Новый титр\nSecond line'})
})
it('keeps an unsaved draft when another edit changes the title and requires an explicit reset',async()=>{
 await start();const input=document.querySelector<HTMLTextAreaElement>('[aria-label="Текст титра Concat"]')!;input.value='Draft';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const changed=project();const track=changed.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text track required');(track.clips[0] as {text:string}).text='External'
 nestedComposition.document=changed;await tick();expect(input.value).toBe('Draft');expect(document.querySelector('[role=alert]')?.textContent).toContain('изменился')
 const buttons=[...document.querySelectorAll('button')];expect(buttons.find(button=>button.textContent==='Сохранить текст титра')!.disabled).toBe(true)
 buttons.find(button=>button.textContent==='Сбросить черновик титра')!.click();await tick();expect(input.value).toBe('External');expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
