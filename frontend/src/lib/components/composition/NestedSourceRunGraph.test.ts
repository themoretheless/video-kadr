import {sourceRunClipboard} from '$lib/state/sourceRunClipboard.svelte'
import {captureSourceRunClipboard,serializeSourceRunClipboard} from '$lib/composition/sourceRunClipboard'
import {sampleConcatSourceRun} from '$lib/composition/concatSourceWindow'
import type {GaussianBlurEffect} from '$lib/composition/concatGaussianBlurAuthoring'
import type {WhiteBalanceEffect} from '$lib/composition/concatWhiteBalanceSourceKeys'
import type {WireConcatParamKey} from '$lib/composition/types'
import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {authState} from '$lib/state/auth.svelte'
import {agentSession} from '$lib/agent/session.svelte'
import {effectScalarProject} from '$lib/composition/fixtures/nativeEffectScalarProject'
import Inspector from './NestedConcatExposureInspector.svelte'
import RunGraph from './SourceKeyRunGraph.svelte'
import RadiusEditor from './ConcatRadiusSourceKeyEditor.svelte'
import WhiteBalanceEditor from './ConcatWhiteBalanceSourceKeyEditor.svelte'
import GainEditor from './ConcatGainSourceKeyEditor.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const clip=()=>nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
function effect(){const value=clip();if(!('effects' in value)||value.effects?.[0]?.kind!=='concat_exposure')throw new Error('Exposure required');return value.effects[0]}
function point(index=1){return document.querySelector<SVGElement>(`[aria-label="shape concat_exposure исходный ключ ${index}"]`)!}
function pointer(target:Element,type:string,x:number,y:number,id=1){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y});Object.defineProperty(event,'pointerId',{value:id});target.dispatchEvent(event)}
beforeEach(()=>{
 sourceRunClipboard.value=null;closeNestedComposition();authState.token='source-run';vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document=effectScalarProject();nestedComposition.activeCompositionId='root';nestedComposition.projectId='source-project';nestedComposition.revision=3;nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-render'}
 Object.assign(clip(),{effects:[{kind:'concat_exposure',parameters:{stops:0},concatKeys:[{at:0,value:-2,ease:'linear'},{at:.5,value:-1,derived:true,ease:[.42,-.5,.58,1]},{at:1,value:0,ease:'out'}]}]})
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();authState.token=null;document.body.innerHTML='';vi.unstubAllGlobals()})
async function open(){component=mount(Inspector,{target:document.body});await tick();const svg=document.querySelector<SVGSVGElement>('[aria-label="shape concat_exposure график исходных ключей"]')!;expect(svg).not.toBeNull();svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180);return svg}
it('moves a source key in normalized clip time with one native commit, keeping arrival easing and derived value metadata',async()=>{
 const before=JSON.stringify(nestedComposition.document),svg=await open(),element=point();pointer(element,'pointerdown',320,100);pointer(svg,'pointermove',170,100);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(Number(point().getAttribute('cx'))).toBe(170)
 pointer(svg,'pointerup',170,100);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(effect().concatKeys?.[1]).toEqual({at:.25,value:-1,derived:true,ease:[.42,-.5,.58,1]});expect(nestedComposition.renderedOutputs).toEqual({});expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const oracle=process.env.NATIVE_SOURCE_RUN_GRAPH_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);redoNestedComposition();await tick();expect(effect().concatKeys?.[1]?.at).toBe(.25)
})
it('edits vertical values without reordering duplicate source keys or changing their easing spelling',async()=>{
 Object.assign(clip(),{effects:[{...effect(),concatKeys:[{at:.9,value:0,ease:'out'},{at:.5,value:-1,ease:'in',derived:true},{at:.5,value:-2,ease:'linear'}]}]});const svg=await open(),element=point();pointer(element,'pointerdown',320,100);pointer(svg,'pointerup',320,90);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));const keys=effect().concatKeys!;expect(keys.map(key=>key.at)).toEqual([.9,.5,.5]);expect(keys[1]!.ease).toBe('in');expect(keys[1]!.value).toBeGreaterThan(-1);expect(keys[1]!.derived).toBeUndefined();expect(keys[0]).toEqual({at:.9,value:0,ease:'out'});expect(keys[2]).toEqual({at:.5,value:-2,ease:'linear'})
})
it.each(['escape','pointercancel','capture','resize','layout','time','session','project','in_place','lock','unmount'] as const)('discards a source graph gesture on %s and ignores a secondary pointer',async reason=>{
 const svg=await open(),element=point();pointer(element,'pointerdown',320,100);pointer(svg,'pointermove',170,100);await tick();pointer(svg,'pointercancel',170,100,2);expect(Number(point().getAttribute('cx'))).toBe(170)
 if(reason==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(reason==='pointercancel')pointer(svg,'pointercancel',170,100)
 if(reason==='capture')element.dispatchEvent(new Event('lostpointercapture'))
 if(reason==='resize')window.dispatchEvent(new Event('resize'))
 if(reason==='layout')svg.getBoundingClientRect=()=>new DOMRect(10,0,640,180)
 if(reason==='time')nestedComposition.timeUs++
 if(reason==='session')authState.token='replacement'
 if(reason==='project')nestedComposition.document=JSON.parse(JSON.stringify(nestedComposition.document))
 if(reason==='in_place')Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{width:130})
 if(reason==='lock')agentSession.lock={owner:'other',reason:'edit',expiresAt:Date.now()+60000}
 if(reason==='unmount'){await unmount(component!);component=undefined}
 const before=JSON.stringify(nestedComposition.document);await tick();pointer(svg,'pointerup',170,100);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('supports source-key keyboard nudges and restores the original graph after native refusal',async()=>{
 await open();point().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',altKey:true,bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(effect().concatKeys?.[1]?.at).toBe(.499)
 vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('source graph refused'));await tick();const before=JSON.stringify(nestedComposition.document);point().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.error).toContain('source graph refused'));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(1)
})

it('refuses a pending native source-key acceptance after the authentication session changes',async()=>{
 let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const svg=await open(),before=JSON.stringify(nestedComposition.document)
 pointer(point(),'pointerdown',320,100);pointer(svg,'pointerup',170,100);await vi.waitFor(()=>expect(finish).toBeDefined());authState.token='replacement';await tick();finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'});expect(nestedHistory.error).toContain('Сессия эффекта')
})

it('draws a finite graph for flat derived values far above ordinary slider bounds',async()=>{
 const keys=[{at:0,value:1e30,derived:true},{at:1,value:1e30,derived:true}];component=mount(RunGraph,{target:document.body,props:{keys,label:'HDR',identity:JSON.stringify(keys),onchange:vi.fn()}});await tick()
 for(const point of document.querySelectorAll('circle')){expect(Number.isFinite(Number(point.getAttribute('cy')))).toBe(true);expect(Number(point.getAttribute('cy'))).toBeGreaterThan(0);expect(Number(point.getAttribute('cy'))).toBeLessThan(180)}
 expect(document.querySelector('path')!.getAttribute('d')).not.toMatch(/NaN|Infinity/)
})

it('inserts a source key with a real graph double click and deletes that exact index with one Undo per action',async()=>{
 const svg=await open(),before=JSON.stringify(nestedComposition.document),background=svg.querySelector('[aria-label="shape concat_exposure добавить исходный ключ"]')!
 background.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:470,clientY:90}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(effect().concatKeys!.map(key=>key.at)).toEqual([0,.5,.75,1]);expect(effect().concatKeys![2]).toMatchObject({ease:[0,0,1,1]});expect(effect().concatKeys![1]).toEqual({at:.5,value:-1,derived:true,ease:[.42,-.5,.58,1]});const inserted=JSON.stringify(nestedComposition.document)
 point(2).dispatchEvent(new KeyboardEvent('keydown',{key:'Delete',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(evaluateNestedFrame).toHaveBeenCalledTimes(2)
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(inserted);redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('retains the original Concat nearest-key insertion rule and supports keyboard insertion on the graph background',async()=>{
 const svg=await open(),background=svg.querySelector('[aria-label="shape concat_exposure добавить исходный ключ"]')!
 background.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:320.3,clientY:90}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(effect().concatKeys).toHaveLength(3);expect(effect().concatKeys![1]!.at).toBeCloseTo(.5005);expect(effect().concatKeys![1]!.derived).toBeUndefined()
 background.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(effect().concatKeys).toHaveLength(3);expect(effect().concatKeys![1]!.at).toBe(.5)
})
it('does not insert or delete source keys while another editor holds the history lock',async()=>{
 const svg=await open(),before=JSON.stringify(nestedComposition.document);agentSession.lock={owner:'other',reason:'edit',expiresAt:Date.now()+60000};await tick()
 svg.querySelector('[aria-label="shape concat_exposure добавить исходный ключ"]')!.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:470,clientY:90}));point().dispatchEvent(new KeyboardEvent('keydown',{key:'Delete',bubbles:true}));await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

it('creates the first source key from an empty graph, deletes the last key and recreates it with Enter',async()=>{
 Object.assign(clip(),{effects:[{kind:'concat_exposure',parameters:{stops:-1}}]});const before=JSON.stringify(nestedComposition.document),svg=await open(),background=svg.querySelector('[aria-label="shape concat_exposure добавить исходный ключ"]')!
 expect(svg.querySelectorAll('circle')).toHaveLength(0);background.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:170,clientY:90}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(effect().concatKeys).toEqual([{at:.25,value:0,ease:[0,0,1,1]}])
 const oracle=process.env.NATIVE_SOURCE_EMPTY_GRAPH_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 point(0).dispatchEvent(new KeyboardEvent('keydown',{key:'Delete',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(effect().concatKeys).toBeUndefined();expect(svg.isConnected).toBe(true)
 background.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));expect(effect().concatKeys).toEqual([{at:.5,value:-1,ease:[0,0,1,1]}]);undoNestedComposition();await tick();expect(effect().concatKeys).toBeUndefined();expect(svg.isConnected).toBe(true)
})

it('uses the radius domain when creating its first graphical source key',async()=>{
 let effect:import('$lib/composition/concatGaussianBlurAuthoring').GaussianBlurEffect={kind:'concat_gaussian_blur',parameters:{radius:10}}
 component=mount(RadiusEditor,{target:document.body,props:{effect,clipId:'radius',onchange:(change:(effect:GaussianBlurEffect)=>GaussianBlurEffect)=>{effect=change(effect)}}});await tick();const svg=document.querySelector<SVGSVGElement>('[aria-label="radius concat_gaussian_blur график исходных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180)
 svg.querySelector('[aria-label="radius concat_gaussian_blur добавить исходный ключ"]')!.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:320,clientY:90}));expect(effect.concatKeys).toEqual([{at:.5,value:25.5,ease:[0,0,1,1]}])
})
it('uses the temperature domain for an empty named source channel',async()=>{
 let effect:import('$lib/composition/concatWhiteBalanceSourceKeys').WhiteBalanceEffect={kind:'concat_white_balance',parameters:{temperature:6500}}
 component=mount(WhiteBalanceEditor,{target:document.body,props:{effect,channel:'temperature',clipId:'white',onchange:(change:(effect:WhiteBalanceEffect)=>WhiteBalanceEffect)=>{effect=change(effect)}}});await tick();const svg=document.querySelector<SVGSVGElement>('[aria-label="white concat_white_balance temperature график исходных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180)
 svg.querySelector('[aria-label="white concat_white_balance temperature добавить исходный ключ"]')!.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,button:0,clientX:320,clientY:90}));expect(effect.concatKeys?.temperature).toEqual([{at:.5,value:6500,ease:[0,0,1,1]}])
})
it('creates an empty gain run from its existing rest level using keyboard insertion',async()=>{
 let keys:readonly import('$lib/composition/types').WireConcatParamKey[]=[]
 component=mount(GainEditor,{target:document.body,props:{keys,rest:1.5,onchange:(next:readonly WireConcatParamKey[])=>{keys=next}}});await tick();document.querySelector('[aria-label="Громкость Concat добавить исходный ключ"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));expect(keys).toMatchObject([{at:.5,value:1.5}])
})

function selectPoint(index:number,toggle=false){const element=point(index),event=new MouseEvent('pointerdown',{bubbles:true,button:0,clientX:100,clientY:90,shiftKey:toggle});Object.defineProperty(event,'pointerId',{value:1});element.dispatchEvent(event);pointer(element.ownerSVGElement!,'pointerup',100,90)}
it('moves a selected source group as one indexed transaction and keeps the group selected for repeated keyboard edits',async()=>{
 const svg=await open();selectPoint(0);await tick();selectPoint(1,true);await tick();expect(point(0).getAttribute('aria-pressed')).toBe('true');expect(point(1).getAttribute('aria-pressed')).toBe('true');expect(point(2).getAttribute('aria-pressed')).toBe('false')
 const before=JSON.stringify(nestedComposition.document);pointer(point(1),'pointerdown',320,100);pointer(svg,'pointermove',380,100);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);pointer(svg,'pointerup',380,100);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(effect().concatKeys![0]!.at).toBeCloseTo(.1);expect(effect().concatKeys![1]!.at).toBeCloseTo(.6);expect(effect().concatKeys![2]!.at).toBe(1);expect(effect().concatKeys![1]!.derived).toBe(true);expect(effect().concatKeys![1]!.ease).toEqual([.42,-.5,.58,1]);expect(point(0).getAttribute('aria-pressed')).toBe('true');expect(point(1).getAttribute('aria-pressed')).toBe('true')
 const oracle=process.env.NATIVE_SOURCE_GROUP_GRAPH_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs,sampledValue:sampleConcatSourceRun(effect().concatKeys!,.25)}))
 point(1).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();expect(effect().concatKeys![0]!.at).toBeCloseTo(.11);expect(effect().concatKeys![1]!.at).toBeCloseTo(.61);expect(point(0).getAttribute('aria-pressed')).toBe('true');expect(evaluateNestedFrame).toHaveBeenCalledTimes(2)
})
it('selects all source indices with Ctrl+A and deletes them in one Undo, including duplicate positions',async()=>{
 const base=effect();Object.assign(clip(),{effects:[{...base,concatKeys:[...base.concatKeys!,{at:.5,value:-2,ease:'linear'}]}]});await open();const before=JSON.stringify(nestedComposition.document)
 point(1).dispatchEvent(new KeyboardEvent('keydown',{key:'a',ctrlKey:true,bubbles:true}));await tick();for(let index=0;index<4;index++)expect(point(index).getAttribute('aria-pressed')).toBe('true')
 point(1).dispatchEvent(new KeyboardEvent('keydown',{key:'Delete',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(effect().concatKeys).toBeUndefined();expect(evaluateNestedFrame).toHaveBeenCalledOnce();undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('refuses a group edit atomically when a later selected value exceeds its source parameter bounds',async()=>{
 Object.assign(clip(),{effects:[{...effect(),concatKeys:[{at:0,value:3.9,ease:'linear'},{at:1,value:4,ease:'linear'}]}]});await open();const before=JSON.stringify(nestedComposition.document)
 point(0).dispatchEvent(new KeyboardEvent('keydown',{key:'a',ctrlKey:true,bubbles:true}));await tick();point(0).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',shiftKey:true,bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.error).not.toBe(''));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

function sourceButton(text:string){return [...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent===text)!}
it('copies a source selection without history writes and pastes it as one native source transaction',async()=>{
 await open();const before=JSON.stringify(nestedComposition.document);selectPoint(0);await tick();sourceButton('Копировать исходные ключи').click();await tick();expect(sourceRunClipboard.value?.keys).toEqual([{at:0,value:-2,ease:'linear'}]);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 sourceButton('Вставить исходные ключи').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(effect().concatKeys![1]).toEqual({at:.5,value:-2,ease:'linear'});expect(effect().concatKeys![0]).toEqual({at:0,value:-2,ease:'linear'});expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const oracle=process.env.NATIVE_SOURCE_CLIPBOARD_GRAPH_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(sourceRunClipboard.value?.keys).toHaveLength(1)
})
it('publishes a cut buffer only after native acceptance and retains the prior buffer on refusal',async()=>{
 await open();const old=captureSourceRunClipboard([{at:0,value:7}],[0]);sourceRunClipboard.value=old;selectPoint(1);await tick();let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));sourceButton('Вырезать исходные ключи').click();await vi.waitFor(()=>expect(finish).toBeDefined());expect(sourceRunClipboard.value).toBe(old);expect(effect().concatKeys).toHaveLength(3);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(effect().concatKeys).toHaveLength(2);expect(sourceRunClipboard.value?.keys).toEqual([{at:0,value:-1,derived:true,ease:[.42,-.5,.58,1]}])
 const accepted=sourceRunClipboard.value,before=JSON.stringify(nestedComposition.document);vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('cut refused'));selectPoint(0);await tick();sourceButton('Вырезать исходные ключи').click();await vi.waitFor(()=>expect(nestedHistory.error).toContain('cut refused'));await tick();expect(sourceRunClipboard.value).toBe(accepted);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(1)
})
it('refuses out-of-domain source paste atomically through native admission',async()=>{
 await open();sourceRunClipboard.value=captureSourceRunClipboard([{at:0,value:99}],[0]);const before=JSON.stringify(nestedComposition.document);vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Exposure out of range'));await tick();sourceButton('Вставить исходные ключи').click();await vi.waitFor(()=>expect(nestedHistory.error).toContain('Exposure out of range'));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
})
it('keeps gain group addresses correct when their move crosses an unselected key',async()=>{
 let keys:readonly WireConcatParamKey[]=[{at:0,value:1},{at:.1,value:2},{at:.2,value:3}];component=mount(GainEditor,{target:document.body,props:{keys,onchange:(next:readonly WireConcatParamKey[])=>{keys=next}}});await tick();const svg=document.querySelector<SVGSVGElement>('[aria-label="Громкость Concat график исходных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180)
 const circles=svg.querySelectorAll('circle');for(const [index,element] of [circles[0]!,circles[1]!].entries()){const event=new MouseEvent('pointerdown',{bubbles:true,button:0,clientX:100,clientY:90,shiftKey:index===1});Object.defineProperty(event,'pointerId',{value:1});element.dispatchEvent(event);pointer(svg,'pointerup',100,90)}
 pointer(circles[0]!,'pointerdown',100,90);pointer(svg,'pointerup',280,90);expect(keys.map(key=>key.value)).toEqual([3,1,2]);expect(keys[1]!.at).toBeCloseTo(.3);expect(keys[2]!.at).toBeCloseTo(.4)
})

it('explicitly reads/writes the bounded source clipboard format through the platform API',async()=>{
 await open();const value=captureSourceRunClipboard([{at:0,value:-2,ease:'out'}],[0]),text=serializeSourceRunClipboard(value),readText=vi.fn().mockResolvedValue(text),writeText=vi.fn().mockResolvedValue(undefined)
 vi.stubGlobal('navigator',Object.create(navigator,{clipboard:{value:{readText,writeText}}}));sourceButton('Прочитать исходные ключи из системного буфера').click();await vi.waitFor(()=>expect(sourceRunClipboard.value).toEqual(value));await tick();sourceButton('Записать исходные ключи в системный буфер').click();await vi.waitFor(()=>expect(writeText).toHaveBeenCalledWith(text));expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('ignores stale platform reads after session replacement and reports permission refusal without replacing the buffer',async()=>{
 await open();let finish!:(text:string)=>void;const readText=vi.fn().mockImplementation(()=>new Promise<string>(resolve=>{finish=resolve}));vi.stubGlobal('navigator',Object.create(navigator,{clipboard:{value:{readText}}}));sourceButton('Прочитать исходные ключи из системного буфера').click();await vi.waitFor(()=>expect(finish).toBeDefined());authState.token='replacement';await tick();finish(serializeSourceRunClipboard(captureSourceRunClipboard([{at:0,value:1}],[0])));await tick();await tick();expect(sourceRunClipboard.value).toBeNull()
 readText.mockRejectedValue(new Error('clipboard denied'));sourceButton('Прочитать исходные ключи из системного буфера').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('clipboard denied'));expect(sourceRunClipboard.value).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

it('does not publish a late platform clipboard read after its source editor unmounts',async()=>{
 await open();let finish!:(text:string)=>void;vi.stubGlobal('navigator',Object.create(navigator,{clipboard:{value:{readText:()=>new Promise<string>(resolve=>{finish=resolve})}}}));sourceButton('Прочитать исходные ключи из системного буфера').click();await vi.waitFor(()=>expect(finish).toBeDefined());await unmount(component!);component=undefined;finish(serializeSourceRunClipboard(captureSourceRunClipboard([{at:0,value:1}],[0])));await tick();await tick();expect(sourceRunClipboard.value).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

it.each(['accept','refuse','session','project'] as const)('settles a native cut after editor destruction on %s without losing the clipboard boundary',async result=>{
 await open();const old=captureSourceRunClipboard([{at:0,value:7}],[0]);sourceRunClipboard.value=old;selectPoint(1);await tick();const before=JSON.stringify(nestedComposition.document)
 let finish!:(value:{entries:[]})=>void,fail!:(error:Error)=>void;vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise((resolve,reject)=>{finish=resolve;fail=reject}));sourceButton('Вырезать исходные ключи').click();await vi.waitFor(()=>expect(finish).toBeDefined());await unmount(component!);component=undefined;expect(sourceRunClipboard.value).toBe(old)
 if(result==='session')authState.token='replacement'
 if(result==='project')closeNestedComposition()
 if(result==='refuse')fail(new Error('cut refused after unmount'));else finish({entries:[]})
 await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));await tick()
 if(result==='accept'){expect(effect().concatKeys).toHaveLength(2);expect(nestedHistory.past).toHaveLength(1);expect(sourceRunClipboard.value?.keys).toEqual([{at:0,value:-1,derived:true,ease:[.42,-.5,.58,1]}]);undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)}
 else{expect(sourceRunClipboard.value).toBe(old);expect(nestedHistory.past).toHaveLength(0);if(result!=='project')expect(JSON.stringify(nestedComposition.document)).toBe(before)}
})
