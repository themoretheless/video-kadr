import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {nestedEffectPreview} from '$lib/state/nestedEffectPreview.svelte'
import {authState} from '$lib/state/auth.svelte'
import {agentSession} from '$lib/agent/session.svelte'
import {nativeScalarChannels} from '$lib/composition/nativeScalarTimeline'
import {scalarClipboardProject} from '$lib/composition/fixtures/nativeScalarClipboardProject'
import Timeline from './NestedScalarTimeline.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();nestedEffectPreview.value=null;authState.token='graph-session';vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 const project=scalarClipboardProject(),content=project.compositions.root!.content;Object.assign(content,{tracks:[content.tracks[0]!]});Object.assign(content.tracks[0]!.clips[0]!,{enabled:true,text:'Graph'})
 nestedComposition.document=project;nestedComposition.activeCompositionId='root';nestedComposition.projectId='graph-project';nestedComposition.revision=3;nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();nestedEffectPreview.value=null;authState.token=null;document.body.innerHTML='';vi.unstubAllGlobals()})
function button(text:string){return [...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>element.textContent===text)!}
function point(tick:number){return document.querySelector<SVGElement>(`[aria-label="Точка графика source x ${tick}"]`)!}
function pointer(target:Element,type:string,x:number,y:number,id=1){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y});Object.defineProperty(event,'pointerId',{value:id});target.dispatchEvent(event)}
function setNumber(value:string){const input=document.querySelector<HTMLInputElement>('[aria-label="Число для значений нативных ключей"]')!;input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}))}
function setMode(mode:string){const menu=document.querySelector<HTMLSelectElement>('[aria-label="Действие со значениями нативных ключей"]')!;menu.value=mode;menu.dispatchEvent(new Event('change',{bubbles:true}))}
async function open(graph=true){component=mount(Timeline,{target:document.body});await tick();button('Выбрать все ключи').click();await tick();if(graph){const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick();const svg=document.querySelector<SVGSVGElement>('[aria-label="График нативных скалярных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180);return svg}}
function curve(){return nativeScalarChannels(nestedComposition.document!,'root').find(channel=>channel.property==='x')!.value.track}
function spline(){const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip)||clip.transform.x.mode!=='keyframes')throw new Error('Curve required');Object.assign(clip.transform.x.track,{interpolation:{kind:'bezier',p1:[1/3,1/3],p2:[2/3,2/3]}})}
it('moves graph points through both axes with a frozen scale, native draft and one Undo/Redo',async()=>{
 const before=JSON.stringify(nestedComposition.document),svg=(await open())!;const element=point(250);pointer(element,'pointerdown',220,150);pointer(svg,'pointermove',270,110);await tick()
 expect(nestedEffectPreview.value).not.toBeNull();const preview=JSON.stringify(nestedEffectPreview.value!.project)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(point(250)).toBe(element)
 expect(document.body.textContent).toContain('Выбрано: 3');pointer(svg,'pointerup',270,110);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000});expect(curve().keyframes.map(key=>[key.tick,key.value])).toEqual([[750,12],[1250,32],[1750,52]]);expect(nestedEffectPreview.value).toBeNull();expect(nestedComposition.renderedOutputs).toEqual({});expect(document.activeElement).toBe(point(750))
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_GRAPH_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'});redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it('moves a Bezier control on the timeline through its original clock and records its owning tangent',async()=>{
 spline();nestedComposition.timeUs=1125000;const before=JSON.stringify(nestedComposition.document),svg=(await open())!
 const handle=document.querySelector<SVGElement>('[aria-label="Ручка графика source x 250 outgoing"]')!;expect(handle).not.toBeNull();pointer(handle,'pointerdown',100,100);pointer(svg,'pointermove',100,60);await tick()
 expect(nestedEffectPreview.value).not.toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);const preview=JSON.stringify(nestedEffectPreview.value!.project)
 pointer(svg,'pointerup',100,60);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(curve().keyframes[0]!.tangents!.outgoing.x).toBeCloseTo(1/3);expect(curve().keyframes[0]!.tangents!.outgoing.y).toBeCloseTo(14/15);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_TANGENT_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
})
it.each(['offset','multiply','set'] as const)('previews and applies a selected %s value transaction without changing timing',async mode=>{
 await open(false);setMode(mode);setNumber(mode==='multiply'?'0.5':'0.25');await tick();const before=JSON.stringify(nestedComposition.document),ticks=curve().keyframes.map(key=>key.tick)
 button('Предпросмотр значений ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull();const preview=JSON.stringify(nestedEffectPreview.value!.project);expect(JSON.stringify(nestedComposition.document)).toBe(before)
 button('Применить значения ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(curve().keyframes.map(key=>key.tick)).toEqual(ticks)
 expect(curve().keyframes.map(key=>key.value)).toEqual(mode==='multiply'?[0,10,20]:mode==='offset'?[0.25,20.25,40.25]:[0.25,0.25,0.25])
})
it('keeps graph marker identity and positions when values are previewed after reverse-order selection',async()=>{
 const svg=(await open())!;button('Снять выделение ключей').click();await tick()
 const a=document.querySelector<HTMLButtonElement>('[aria-label="source x ключ 750"]')!,b=document.querySelector<HTMLButtonElement>('[aria-label="source x ключ 250"]')!;a.click();b.dispatchEvent(new MouseEvent('click',{bubbles:true,ctrlKey:true}));await tick()
 const x=point(250).getAttribute('cx');setNumber('1');await tick();button('Предпросмотр значений ключей').click();await tick();expect(point(250).getAttribute('cx')).toBe(x);expect(svg.querySelectorAll('[data-native-graph-key]')).toHaveLength(3)
})
it.each(['escape','pointercancel','capture','clock','scope','session','project','in_place','lock','close','unmount'] as const)('cancels a graph gesture on %s',async kind=>{
 const svg=(await open())!,element=point(250);pointer(element,'pointerdown',220,150);pointer(svg,'pointermove',270,110);await tick();expect(nestedEffectPreview.value).not.toBeNull()
 if(kind==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(kind==='pointercancel')pointer(svg,'pointercancel',270,110)
 if(kind==='capture')element.dispatchEvent(new Event('lostpointercapture'))
 if(kind==='clock')nestedComposition.timeUs++
 if(kind==='scope')nestedComposition.activeCompositionId='other'
 if(kind==='session')authState.token='other-session'
 if(kind==='project')nestedComposition.document=JSON.parse(JSON.stringify(nestedComposition.document))
 if(kind==='in_place')Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{width:130})
 if(kind==='lock')agentSession.lock={owner:'agent',reason:'editing',expiresAt:Date.now()+60000}
 if(kind==='close'){const details=svg.closest('details')!;details.open=false;details.dispatchEvent(new Event('toggle'))}
 if(kind==='unmount'){await unmount(component!);component=undefined}
 const before=JSON.stringify(nestedComposition.document);await tick();expect(nestedEffectPreview.value).toBeNull();pointer(svg,'pointerup',270,110);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('ignores a second pointer and clears a frame-snapped no-op gesture',async()=>{
 const svg=(await open())!,element=point(250);pointer(element,'pointerdown',220,150);pointer(svg,'pointermove',225,150);await tick();expect(nestedEffectPreview.value).not.toBeNull()
 pointer(svg,'pointercancel',225,150,2);await tick();expect(nestedEffectPreview.value).not.toBeNull();point(750).dispatchEvent(new Event('lostpointercapture'));await tick();expect(nestedEffectPreview.value).not.toBeNull()
 pointer(svg,'pointerup',225,150);await tick();expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('guards value parameters before deferred cleanup and discards a pending native graph acceptance after cancel',async()=>{
 await open();setNumber('1');await tick();button('Предпросмотр значений ключей').click();await tick();setNumber('2');button('Применить значения ключей').click();await tick();expect(evaluateNestedFrame).not.toHaveBeenCalled()
 let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));button('Предпросмотр значений ключей').click();await tick();const before=JSON.stringify(nestedComposition.document);button('Применить значения ключей').click();await vi.waitFor(()=>expect(finish).toBeDefined());button('Отменить изменение графика').click();await tick();expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![2]!.aborted).toBe(true);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull()
})
it('restores accepted tangent input values and the original frame after native refusal',async()=>{
 spline();await open();vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Graph tangent rejected'))
 const field=()=>document.querySelector<HTMLInputElement>('[aria-label="График source x 250 tangent outgoing y"]')!,before=JSON.stringify(nestedComposition.document);field().value='0.9';field().dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(document.body.textContent).toContain('Graph tangent rejected'));await tick()
 expect(Number(field().value)).toBeCloseTo(1/3);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
})
it('supports value and time keyboard gestures without losing the authored sample clock',async()=>{
 await open();point(250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',shiftKey:true,bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(curve().keyframes.map(key=>key.value)).toEqual([10,30,50]);point(250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(curve().keyframes.map(key=>key.tick)).toEqual([750,1250,1750]);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
})

it('selects and deletes graph-channel keys with scoped shortcuts as one transaction',async()=>{
 await open();point(250).dispatchEvent(new KeyboardEvent('keydown',{key:'a',ctrlKey:true,bubbles:true}));await tick();expect(document.body.textContent).toContain('Выбрано: 3')
 point(250).dispatchEvent(new KeyboardEvent('keydown',{key:'Delete',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(nativeScalarChannels(nestedComposition.document!,'root').map(channel=>channel.property)).toEqual(['opacity'])
 undoNestedComposition();await tick();expect(curve().keyframes).toHaveLength(3)
})
it('edits Auto and continuity in the integrated tangent controls while retaining the clock',async()=>{
 spline();await open();const mode=document.querySelector<HTMLSelectElement>('[aria-label="График source x 750 tangent mode"]')!;mode.value='auto';mode.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(curve().keyframes[1]!.tangents).toMatchObject({incoming:{mode:'auto'},outgoing:{mode:'auto'}})
 const continuity=document.querySelector<HTMLSelectElement>('[aria-label="График source x 750 tangent continuity"]')!;continuity.value='smooth';continuity.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(nestedHistory.past).toHaveLength(1);expect(evaluateNestedFrame).toHaveBeenCalledOnce();continuity.value='broken';continuity.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));expect(curve().keyframes[1]!.tangents?.continuity).toBe('broken');expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
})

it('selects graph points by both screen axes without editing the document, then edits only that subset',async()=>{
 const svg=(await open())!,surface=document.querySelector('[aria-label="Выделить рамкой на графике"]')!,before=JSON.stringify(nestedComposition.document)
 pointer(surface,'pointerdown',210,70);pointer(svg,'pointermove',330,100);await tick()
 expect(point(250).getAttribute('aria-pressed')).toBe('false');expect(point(750).getAttribute('aria-pressed')).toBe('true');expect(point(1250).getAttribute('aria-pressed')).toBe('false')
 expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 pointer(svg,'pointerup',330,100);await tick();expect(document.body.textContent).toContain('Выбрано: 1');expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 point(750).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(curve().keyframes.map(key=>key.value)).toEqual([0,21,40]);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
})
it.each(['escape','pointercancel','resize','layout','session'] as const)('discards graph box selection on %s and ignores secondary pointers',async reason=>{
 const svg=(await open())!,surface=document.querySelector('[aria-label="Выделить рамкой на графике"]')!,before=JSON.stringify(nestedComposition.document)
 pointer(surface,'pointerdown',330,100);pointer(svg,'pointermove',210,70);await tick();pointer(svg,'pointerup',210,70,2);await tick();expect(document.body.textContent).toContain('Выбрано: 6')
 if(reason==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(reason==='pointercancel')pointer(svg,'pointercancel',210,70)
 if(reason==='resize')window.dispatchEvent(new Event('resize'))
 if(reason==='layout')svg.getBoundingClientRect=()=>new DOMRect(0,0,320,180)
 if(reason==='session')authState.token='changed'
 await tick();pointer(svg,'pointerup',210,70);await tick();expect(document.body.textContent).toContain(`Выбрано: ${reason==='session'?0:6}`);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it.each([{shiftKey:true,count:6},{ctrlKey:true,count:5},{altKey:true,count:5}])('combines graph boxes with the existing multichannel selection using %j',async({count,...modifier})=>{
 const svg=(await open())!,surface=document.querySelector('[aria-label="Выделить рамкой на графике"]')!
 const event=new MouseEvent('pointerdown',{bubbles:true,button:0,clientX:210,clientY:70,...modifier});Object.defineProperty(event,'pointerId',{value:1});surface.dispatchEvent(event)
 pointer(svg,'pointerup',330,100);await tick();expect(document.body.textContent).toContain(`Выбрано: ${count}`);expect(nestedHistory.past).toHaveLength(0)
})


function graphFrames(){
 let serial=0,stamp=performance.now();const frames=new Map<number,FrameRequestCallback>()
 vi.stubGlobal('requestAnimationFrame',(callback:FrameRequestCallback)=>{frames.set(++serial,callback);return serial})
 vi.stubGlobal('cancelAnimationFrame',(id:number)=>{frames.delete(id)})
 return {count:()=>frames.size,step:async(count=1)=>{for(let index=0;index<count;index++){stamp=Math.max(stamp,performance.now())+50;const batch=[...frames];frames.clear();for(const [,callback] of batch)callback(stamp);await tick()}}}
}
async function narrowGraph(){
 const svg=(await open())!
 for(const [label,value] of [['Начало видимой области ключей','0.9'],['Конец видимой области ключей','1.1']]){const input=document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;input.value=value!;input.dispatchEvent(new Event('input',{bubbles:true}))}
 await tick();button('Применить область ключей').click();await tick();return svg
}
it('edge-pans point drags on the shared viewport with a frozen value axis, authored clocks and one accepted edit',async()=>{
 const frames=graphFrames(),before=JSON.stringify(nestedComposition.document),svg=await narrowGraph(),element=point(250)
 pointer(element,'pointerdown',320,150);pointer(svg,'pointermove',610,110);await frames.step(30);expect(point(250)).toBe(element);expect(frames.count()).toBe(1)
 expect(Number(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent!.split('–')[0])).toBeGreaterThan(1.1)
 expect(nestedEffectPreview.value).not.toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
 pointer(svg,'pointerup',610,110);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(frames.count()).toBe(0);expect(curve().keyframes.map(key=>[key.tick,key.value])).toEqual([[750,12],[1250,32],[1750,52]]);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000});expect(document.activeElement).toBe(point(750))
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_GRAPH_AUTOPAN_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedEffectPreview.value).toBeNull()
})
it('edge-pans a graph box over initially invisible keys while preserving its value bounds and keeping selection ephemeral',async()=>{
 const frames=graphFrames(),before=JSON.stringify(nestedComposition.document),svg=await narrowGraph(),surface=document.querySelector('[aria-label="Выделить рамкой на графике"]')!
 pointer(surface,'pointerdown',300,170);pointer(svg,'pointermove',610,70);await frames.step(62);expect(frames.count()).toBe(1)
 // The rectangle includes values 0 and 20, but excludes 40 despite reaching its time.
 expect(point(1250).getAttribute('aria-pressed')).toBe('false');expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 pointer(svg,'pointerup',610,70);await tick();expect(frames.count()).toBe(0);expect(document.body.textContent).toContain('Выбрано: 2');expect(evaluateNestedFrame).not.toHaveBeenCalled()
 setNumber('1');await tick();button('Предпросмотр значений ключей').click();await tick();button('Применить значения ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(curve().keyframes.map(key=>key.value)).toEqual([1,21,40])
})
it('keeps the captured Bezier handle mounted and converts panned time into its owning normalized control',async()=>{
 spline();const frames=graphFrames(),before=JSON.stringify(nestedComposition.document),svg=await narrowGraph(),element=document.querySelector<SVGElement>('[aria-label="Ручка графика source x 250 outgoing"]')!
 pointer(element,'pointerdown',570,124);pointer(svg,'pointermove',610,124);await tick();const value=()=>nativeScalarChannels(nestedEffectPreview.value!.project,'root').find(channel=>channel.property==='x')!.value.track.keyframes[0]!.tangents!.outgoing.x
 const initial=value();await frames.step(5);expect(value()).toBeGreaterThan(initial);expect(document.querySelector('[aria-label="Ручка графика source x 250 outgoing"]')).toBe(element);expect(JSON.stringify(nestedComposition.document)).toBe(before)
 pointer(svg,'pointerup',610,124);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(frames.count()).toBe(0);expect(curve().keyframes[0]!.tangents!.outgoing.x).toBeGreaterThan(initial);expect(curve().sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
})
it.each(['key','box','tangent'] as const)('cancels scheduled graph %s edge pan without edits or selection leakage',async kind=>{
 if(kind==='tangent')spline()
 const frames=graphFrames(),before=JSON.stringify(nestedComposition.document),svg=await narrowGraph(),element=kind==='key'?point(250):document.querySelector(kind==='box'?'[aria-label="Выделить рамкой на графике"]':'[aria-label="Ручка графика source x 250 outgoing"]')!
 pointer(element,'pointerdown',kind==='tangent'?570:320,150);pointer(svg,'pointermove',610,110);await frames.step(3);pointer(svg,'pointercancel',610,110,2);expect(frames.count()).toBe(1)
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}));await tick();expect(frames.count()).toBe(0);pointer(svg,'pointerup',610,110);await frames.step();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(document.body.textContent).toContain(`Выбрано: ${kind==='key'?3:6}`)
})
it.each(['resize','layout','view','session','capture','close','unmount'] as const)('stops scheduled graph edge pan on %s',async reason=>{
 const frames=graphFrames(),before=JSON.stringify(nestedComposition.document),svg=await narrowGraph(),element=point(250);pointer(element,'pointerdown',320,150);pointer(svg,'pointermove',610,110);await frames.step(2)
 if(reason==='resize')window.dispatchEvent(new Event('resize'))
 if(reason==='layout')svg.getBoundingClientRect=()=>new DOMRect(20,0,640,180)
 if(reason==='view')button('Показать все ключи').click()
 if(reason==='session')authState.token='replacement'
 if(reason==='capture')element.dispatchEvent(new Event('lostpointercapture'))
 if(reason==='close'){const details=svg.closest('details')!;details.open=false;details.dispatchEvent(new Event('toggle'))}
 if(reason==='unmount'){await unmount(component!);component=undefined}
 await tick();await frames.step();expect(frames.count()).toBe(0);pointer(svg,'pointerup',610,110);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
