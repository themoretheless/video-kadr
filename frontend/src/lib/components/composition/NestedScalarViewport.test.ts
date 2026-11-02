import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {nestedEffectPreview} from '$lib/state/nestedEffectPreview.svelte'
import {authState} from '$lib/state/auth.svelte'
import {agentSession} from '$lib/agent/session.svelte'
import {scalarClipboardProject} from '$lib/composition/fixtures/nativeScalarClipboardProject'
import Timeline from './NestedScalarTimeline.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();nestedEffectPreview.value=null;authState.token='viewport-session';vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 const project=scalarClipboardProject(),content=project.compositions.root!.content;Object.assign(content,{tracks:[content.tracks[0]!]});Object.assign(content.tracks[0]!.clips[0]!,{enabled:true,text:'Viewport'})
 nestedComposition.document=project;nestedComposition.activeCompositionId='root';nestedComposition.projectId='viewport-project';nestedComposition.revision=3;nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();nestedEffectPreview.value=null;authState.token=null;document.body.innerHTML='';vi.unstubAllGlobals()})
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>element.textContent===text)!
const key=(property:string,tick:number)=>document.querySelector<HTMLButtonElement>(`[aria-label="source ${property} ключ ${tick}"]`)!
const surface=()=>document.querySelector<HTMLButtonElement>('[aria-label="Выделить диапазон source x"]')!
function pointer(element:Element,type:string,x:number,y:number,extra:MouseEventInit={},id=1){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y,...extra});Object.defineProperty(event,'pointerId',{value:id});element.dispatchEvent(event)}
function input(label:string,value:string){const element=document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;element.value=value;element.dispatchEvent(new Event('input',{bubbles:true}))}
// Happy DOM's WheelEvent extends UIEvent; provide the standard MouseEvent fields.
function wheel(options:WheelEventInit){const event=new WheelEvent('wheel',options);Object.defineProperties(event,{ctrlKey:{value:options.ctrlKey??false},metaKey:{value:options.metaKey??false},shiftKey:{value:options.shiftKey??false},clientX:{value:options.clientX??0}});return event}
function mode(value:string){const element=document.querySelector<HTMLSelectElement>('[aria-label="Режим выделения нативных ключей"]')!;element.value=value;element.dispatchEvent(new Event('change',{bubbles:true}))}
async function open(){component=mount(Timeline,{target:document.body});await tick();let top=100;for(const row of document.querySelectorAll<HTMLElement>('.ruler')){const y=top;row.getBoundingClientRect=()=>new DOMRect(0,y,300,32);top+=40}document.querySelector<HTMLElement>('.lanes')!.getBoundingClientRect=()=>new DOMRect(0,100,350,72)}
it('selects a mixed-clock rectangle without authoring/history changes and supports the subsequent group edit after zoom',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();button('Приблизить ключи по курсору').click();await tick();expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.750–2.250');expect(Number.parseFloat(key('x',250).style.left)).toBeCloseTo(100/6)
 const background=surface();pointer(background,'pointerdown',40,100);pointer(background,'pointermove',160,180);await tick();expect(document.querySelector('[aria-label="Рамка выделения ключей"]')).not.toBeNull();expect(document.body.textContent).toContain('Выбрано: 5');expect(key('opacity',24).getAttribute('aria-pressed')).toBe('false')
 pointer(background,'pointerup',160,180);await tick();expect(document.querySelector('[aria-label="Рамка выделения ключей"]')).toBeNull();expect(document.activeElement).toBe(key('x',250));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_VIEWPORT_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
})
it.each([['shift',6],['ctrl',1],['alt',1]] as const)('combines a marquee with %s selection semantics',async(kind,count)=>{
 await open();if(kind==='shift')key('opacity',24).click();else button('Выбрать все ключи').click();await tick()
 const target=surface(),extra={shiftKey:kind==='shift',ctrlKey:kind==='ctrl',altKey:kind==='alt'};pointer(target,'pointerdown',90,100,extra);pointer(target,'pointermove',160,180,extra);pointer(target,'pointerup',160,180,extra);await tick()
 expect(document.body.textContent).toContain(`Выбрано: ${count}`);expect(key('opacity',24).getAttribute('aria-pressed')).toBe('true');expect(nestedHistory.past).toHaveLength(0)
})
it('selects time ranges in both directions with replace/add/toggle/subtract modes',async()=>{
 await open();input('Начало выделения ключей','1.5');input('Конец выделения ключей','1');await tick();button('Выделить ключи по времени').click();await tick();expect(document.body.textContent).toContain('Выбрано: 5')
 input('Начало выделения ключей','2');input('Конец выделения ключей','2');mode('add');await tick();button('Выделить ключи по времени').click();await tick();expect(document.body.textContent).toContain('Выбрано: 6')
 mode('toggle');await tick();button('Выделить ключи по времени').click();await tick();expect(document.body.textContent).toContain('Выбрано: 5')
 input('Начало выделения ключей','1');input('Конец выделения ключей','1.5');mode('subtract');await tick();button('Выделить ключи по времени').click();await tick();expect(document.body.textContent).toContain('Выбрано: 0');expect(nestedHistory.past).toHaveLength(0)
})
it.each(['escape','pointercancel','capture','time','session','project','lock','resize','view','layout','unmount','foreign'] as const)('restores prior selection on marquee %s cancellation',async kind=>{
 await open();key('opacity',24).click();await tick();const target=surface();pointer(target,'pointerdown',90,100);pointer(target,'pointermove',160,180);await tick();expect(document.body.textContent).toContain('Выбрано: 5')
 let foreign:typeof nestedEffectPreview.value=null
 if(kind==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(kind==='pointercancel')pointer(target,'pointercancel',160,180)
 if(kind==='capture')target.dispatchEvent(new Event('lostpointercapture'))
 if(kind==='time')nestedComposition.timeUs++
 if(kind==='session')authState.token='other-session'
 if(kind==='project')nestedComposition.document=JSON.parse(JSON.stringify(nestedComposition.document))
 if(kind==='lock')agentSession.lock={owner:'agent',reason:'editing',expiresAt:Date.now()+60000}
 if(kind==='resize')window.dispatchEvent(new Event('resize'))
 if(kind==='view')button('Приблизить ключи по курсору').click()
 if(kind==='layout'){target.parentElement!.getBoundingClientRect=()=>new DOMRect(20,100,300,32);pointer(target,'pointermove',170,180)}
 if(kind==='unmount'){await unmount(component!);component=undefined}
 if(kind==='foreign'){foreign={owner:Symbol('foreign'),gesture:{},base:nestedComposition.document!,project:nestedComposition.document!,timeUs:nestedComposition.timeUs,compositionId:'root',token:authState.token!};nestedEffectPreview.value=foreign}
 const expected=JSON.stringify(nestedComposition.document);await tick();pointer(target,'pointerup',160,180);await tick();if(kind!=='unmount'&&kind!=='session')expect(document.body.textContent).toContain('Выбрано: 1')
 expect(document.querySelector('[aria-label="Рамка выделения ключей"]')).toBeNull();expect(nestedEffectPreview.value).toBe(foreign);expect(JSON.stringify(nestedComposition.document)).toBe(expected);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('pans, fits and applies explicit viewport bounds without changing the project or hiding focused edited keys',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();button('Приблизить ключи по курсору').click();await tick();button('Область ключей позже').click();await tick();expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('1.500–3.000');expect(key('x',250)).toBeNull()
 button('Область ключей раньше').click();await tick();expect(key('x',250)).not.toBeNull();key('x',250).click();await tick();button('Показать выделенные ключи').click();await tick();expect(key('x',250)).not.toBeNull()
 button('Показать все ключи').click();await tick();input('Начало видимой области ключей','1');input('Конец видимой области ключей','1.2');await tick();button('Применить область ключей').click();await tick();expect(key('x',750)).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(document.body.textContent).toContain('уже есть другой ключ'));expect(nestedHistory.past).toHaveLength(0)
 input('Начало видимой области ключей','3');input('Конец видимой области ключей','2');await tick();button('Применить область ключей').click();await tick();expect(document.body.textContent).toContain('Некорректная видимая область');expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('ignores a secondary pointer and supports keyboard selection of a visible channel',async()=>{
 await open();const target=surface();pointer(target,'pointerdown',90,100);pointer(target,'pointermove',160,180);pointer(target,'pointercancel',160,180,{},2);await tick();expect(document.querySelector('[aria-label="Рамка выделения ключей"]')).not.toBeNull();pointer(target,'pointerup',160,180);await tick();button('Снять выделение ключей').click();await tick();target.click();await tick();expect(document.body.textContent).toContain('Выбрано: 3')
})

it('reveals the moved key and restores its focus when a committed edit crosses a custom viewport edge',async()=>{
 await open();input('Начало видимой области ключей','1');input('Конец видимой области ключей','1.2');await tick();button('Применить область ключей').click();await tick();button('Выбрать все ключи').click();await tick()
 key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('1.150–1.350');expect(key('x',750)).not.toBeNull();expect(document.activeElement).toBe(key('x',750));expect(Number.parseFloat(key('x',750).style.left)).toBeCloseTo(50)
})
it('cancels a graph gesture when viewport coordinates change before release',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();button('Выбрать все ключи').click();await tick()
 const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 const svg=document.querySelector<SVGSVGElement>('[aria-label="График нативных скалярных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180)
 const point=document.querySelector<SVGElement>('[aria-label="Точка графика source x 250"]')!;pointer(point,'pointerdown',220,150);pointer(svg,'pointermove',220,130);await tick();expect(nestedEffectPreview.value).not.toBeNull()
 button('Приблизить ключи по курсору').click();await tick();expect(nestedEffectPreview.value).toBeNull();pointer(svg,'pointerup',220,130);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('resets the view on project-context changes and applies the chosen selection mode to marquee gestures',async()=>{
 await open();key('opacity',24).click();await tick();mode('add');await tick();const target=surface();pointer(target,'pointerdown',90,100);pointer(target,'pointermove',160,180);pointer(target,'pointerup',160,180);await tick();expect(document.body.textContent).toContain('Выбрано: 6')
 button('Приблизить ключи по курсору').click();await tick();nestedComposition.projectId='another-project';await tick();expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.000–3.000');expect(document.body.textContent).toContain('Выбрано: 0');expect(nestedHistory.past).toHaveLength(0)
})

it('uses the full graph width for subsecond views and excludes offscreen tangent controls from keyboard focus',async()=>{
 await open();const source=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in source)||source.transform.x.mode!=='keyframes')throw new Error('Curve required')
 Object.assign(source.transform.x.track,{interpolation:{kind:'bezier',p1:[1/3,1/3],p2:[2/3,2/3]}})
 input('Начало видимой области ключей','1');input('Конец видимой области ключей','1.2');await tick();button('Применить область ключей').click();await tick()
 const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 const svg=document.querySelector<SVGSVGElement>('[aria-label="График нативных скалярных ключей"]')!
 expect(svg.querySelector('g')!.getAttribute('transform')).toBe('translate(-2980 10)');expect(svg.querySelectorAll('[data-native-graph-key]')).toHaveLength(1)
 expect(svg.querySelectorAll('[aria-label^="Ручка графика"]')).toHaveLength(2)
 const handle=svg.querySelector<SVGElement>('[aria-label="Ручка графика source x 250 outgoing"]')!;expect(Number(handle.getAttribute('cx'))).toBeCloseTo(3250)
 expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
})

it('zooms around the wheel pointer and pans without intercepting ordinary vertical page scrolling',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();const row=surface().parentElement!,left=key('x',250).style.left
 const normal=wheel({deltaY:30,clientX:100,bubbles:true,cancelable:true});row.dispatchEvent(normal);await tick();expect(normal.defaultPrevented).toBe(false)
 const zoom=wheel({deltaY:-Math.log(2)/0.002,clientX:100,ctrlKey:true,bubbles:true,cancelable:true});row.dispatchEvent(zoom);await tick();expect(zoom.defaultPrevented).toBe(true);expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.500–2.000');expect(Number.parseFloat(key('x',250).style.left)).toBeCloseTo(Number.parseFloat(left))
 row.dispatchEvent(wheel({deltaY:30,shiftKey:true,bubbles:true,cancelable:true}));await tick();expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.650–2.150')
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('shares pointer-anchored wheel navigation between the graph and the lane rulers',async()=>{
 await open();const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 const svg=document.querySelector<SVGSVGElement>('[aria-label="График нативных скалярных ключей"]')!;svg.getBoundingClientRect=()=>new DOMRect(0,0,640,180)
 const event=wheel({deltaY:-Math.log(2)/0.002,clientX:220,metaKey:true,bubbles:true,cancelable:true});svg.dispatchEvent(event);await tick();expect(event.defaultPrevented).toBe(true)
 expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.500–2.000');expect(Number.parseFloat(key('x',250).style.left)).toBeCloseTo(100/3);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

it('uses each visible marker vertical center instead of assuming it matches the row center',async()=>{
 await open();key('x',250).getBoundingClientRect=()=>new DOMRect(92,102,16,16)
 const target=surface();pointer(target,'pointerdown',90,101);pointer(target,'pointermove',110,112);pointer(target,'pointerup',110,112);await tick()
 expect(document.body.textContent).toContain('Выбрано: 1');expect(key('x',250).getAttribute('aria-pressed')).toBe('true');expect(key('x',750).getAttribute('aria-pressed')).toBe('false');expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})


it('restores authored key selection and its custom viewport together across Undo and Redo',async()=>{
 await open();input('Начало видимой области ключей','1');input('Конец видимой области ключей','1.2');await tick();button('Применить область ключей').click();await tick();button('Выбрать все ключи').click();await tick()
 key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const range=()=>document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent
 expect(range()).toContain('1.150–1.350');expect(document.body.textContent).toContain('Выбрано: 6')
 button('Снять выделение ключей').click();button('Показать все ключи').click();await tick()
 undoNestedComposition();await tick();expect(range()).toContain('1.000–1.200');expect(document.body.textContent).toContain('Выбрано: 6');expect(key('x',250).getAttribute('aria-pressed')).toBe('true')
 redoNestedComposition();await tick();expect(range()).toContain('0.000–3.000');expect(document.body.textContent).toContain('Выбрано: 0');expect(key('x',1750)).not.toBeNull()
 undoNestedComposition();await tick();expect(range()).toContain('1.000–1.200');expect(document.body.textContent).toContain('Выбрано: 6');expect(evaluateNestedFrame).toHaveBeenCalledOnce();expect(nestedEffectPreview.value).toBeNull()
})
it('restores selected deleted channels on Undo and clears them again on Redo',async()=>{
 await open();button('Выбрать все ключи').click();await tick();button('Удалить выбранные ключи').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(surface()).toBeNull()
 undoNestedComposition();await tick();expect(document.body.textContent).toContain('Выбрано: 6');expect(key('x',250).getAttribute('aria-pressed')).toBe('true');expect(key('opacity',24).getAttribute('aria-pressed')).toBe('true')
 redoNestedComposition();await tick();expect(surface()).toBeNull();expect(nestedHistory.past).toHaveLength(1);expect(evaluateNestedFrame).toHaveBeenCalledOnce()
})
it.each(['session','remount'] as const)('does not revive stale scalar editor state after %s replacement',async kind=>{
 await open();button('Выбрать все ключи').click();await tick();key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 if(kind==='session'){authState.token='replacement-session';await tick()}
 else {await unmount(component!);component=undefined;await open()}
 undoNestedComposition();await tick();expect(document.body.textContent).toContain('Выбрано: 0');expect(key('x',250).getAttribute('aria-pressed')).toBe('false')
})


function animationFrames(){
 let serial=0,stamp=performance.now();const frames=new Map<number,FrameRequestCallback>()
 vi.stubGlobal('requestAnimationFrame',(callback:FrameRequestCallback)=>{frames.set(++serial,callback);return serial})
 vi.stubGlobal('cancelAnimationFrame',(id:number)=>{frames.delete(id)})
 return {count:()=>frames.size,step:async(count=1)=>{for(let index=0;index<count;index++){stamp=Math.max(stamp,performance.now())+50;const batch=[...frames];frames.clear();for(const [,callback] of batch)callback(stamp);await tick()}}}
}
async function narrowDrag(){
 await open();input('Начало видимой области ключей','0.9');input('Конец видимой области ключей','1.1');await tick();button('Применить область ключей').click();await tick();button('Выбрать все ключи').click();await tick()
 const element=key('x',250);pointer(element,'pointerdown',150,100);pointer(element,'pointermove',290,100);await tick();return element
}
it('edge-pans continuously under a stationary pointer, retaining capture markers and original key clocks in one transaction',async()=>{
 const frames=animationFrames(),before=JSON.stringify(nestedComposition.document),element=await narrowDrag()
 expect(frames.count()).toBe(1);await frames.step(30);expect(key('x',250)).toBe(element)
 const label=document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent!;expect(Number(label.split('–')[0])).toBeGreaterThan(1.1)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedEffectPreview.value).not.toBeNull()
 pointer(element,'pointerup',290,100);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(frames.count()).toBe(0);expect(document.activeElement).toBe(key('x',750));expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip)||clip.transform.x.mode!=='keyframes')throw new Error('X keys required')
 expect(clip.transform.x.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1750]);expect(clip.transform.x.track.sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_AUTOPAN_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedEffectPreview.value).toBeNull()
})
it.each(['escape','pointercancel','capture','resize','layout','session','lock','unmount'] as const)('stops edge pan and discards its draft on %s',async reason=>{
 const frames=animationFrames(),before=JSON.stringify(nestedComposition.document),element=await narrowDrag();await frames.step(2);expect(frames.count()).toBe(1)
 pointer(element,'pointercancel',290,100,{},2);expect(frames.count()).toBe(1)
 if(reason==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(reason==='pointercancel')pointer(element,'pointercancel',290,100)
 if(reason==='capture')element.dispatchEvent(new Event('lostpointercapture'))
 if(reason==='resize')window.dispatchEvent(new Event('resize'))
 if(reason==='layout')element.parentElement!.getBoundingClientRect=()=>new DOMRect(20,100,300,32)
 if(reason==='session')authState.token='replacement'
 if(reason==='lock')agentSession.lock={owner:'agent',reason:'edit',expiresAt:Date.now()+60000}
 if(reason==='unmount'){await unmount(component!);component=undefined}
 await tick();await frames.step();expect(frames.count()).toBe(0);pointer(element,'pointerup',290,100);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('stops scheduled edge pan when the pointer returns to the center or the viewport reaches its extent',async()=>{
 const frames=animationFrames(),element=await narrowDrag();pointer(element,'pointermove',150,100);await tick();expect(frames.count()).toBe(0)
 pointer(element,'pointermove',-10,100);await tick();await frames.step(110);expect(frames.count()).toBe(0);expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.000–0.200');expect(nestedHistory.past).toHaveLength(0)
})

async function narrowMarquee(extra:MouseEventInit={},startX=140){
 await open();input('Начало видимой области ключей','0.9');input('Конец видимой области ключей','1.1');await tick();button('Применить область ключей').click();await tick()
 const element=surface();pointer(element,'pointerdown',startX,100,extra);pointer(element,'pointermove',290,180,extra);await tick();return element
}
it('edge-pans a mixed-clock marquee across initially hidden keys without history or preview writes, then edits the accumulated group',async()=>{
 const frames=animationFrames(),before=JSON.stringify(nestedComposition.document),element=await narrowMarquee();expect(document.body.textContent).toContain('Выбрано: 2')
 await frames.step(62);expect(document.body.textContent).toContain('Выбрано: 5');expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before)
 const overlay=document.querySelector<HTMLElement>('[aria-label="Рамка выделения ключей"]')!;expect(Number.parseFloat(overlay.style.left)).toBe(0);expect(Number.parseFloat(overlay.style.width)).toBe(290)
 pointer(element,'pointerup',290,180);await tick();expect(frames.count()).toBe(0);expect(document.body.textContent).toContain('Выбрано: 5');expect(document.activeElement).toBe(key('x',1250));expect(nestedHistory.past).toHaveLength(0)
 key('x',1250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_MARQUEE_AUTOPAN_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(document.body.textContent).toContain('Выбрано: 5')
})
it.each(['escape','pointercancel','capture','resize','layout','other_row','session','lock','view','unmount'] as const)('stops marquee edge pan on %s without committing provisional selection',async reason=>{
 const frames=animationFrames(),before=JSON.stringify(nestedComposition.document),element=await narrowMarquee();await frames.step(5);expect(frames.count()).toBe(1)
 pointer(element,'pointercancel',290,180,{},2);expect(frames.count()).toBe(1)
 if(reason==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(reason==='pointercancel')pointer(element,'pointercancel',290,180)
 if(reason==='capture')element.dispatchEvent(new Event('lostpointercapture'))
 if(reason==='resize')window.dispatchEvent(new Event('resize'))
 if(reason==='layout')element.parentElement!.getBoundingClientRect=()=>new DOMRect(20,100,300,32)
 if(reason==='other_row')document.querySelectorAll<HTMLElement>('.ruler')[1]!.getBoundingClientRect=()=>new DOMRect(0,150,300,32)
 if(reason==='session')authState.token='replacement'
 if(reason==='lock')agentSession.lock={owner:'agent',reason:'edit',expiresAt:Date.now()+60000}
 if(reason==='view')button('Показать все ключи').click()
 if(reason==='unmount'){await unmount(component!);component=undefined}
 await tick();await frames.step();expect(frames.count()).toBe(0);pointer(element,'pointerup',290,180);await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).not.toHaveBeenCalled()
 if(reason!=='unmount')expect(document.body.textContent).toContain('Выбрано: 0')
})
it.each([{ctrlKey:true,count:1},{altKey:true,count:1},{shiftKey:true,count:6}])('combines the full accumulated edge-pan range with prior selection using %j',async({count,...extra})=>{
 const frames=animationFrames();await open();button('Выбрать все ключи').click();await tick();input('Начало видимой области ключей','0.9');input('Конец видимой области ключей','1.1');await tick();button('Применить область ключей').click();await tick()
 const element=surface();pointer(element,'pointerdown',140,100,extra);pointer(element,'pointermove',290,180,extra);await frames.step(62);pointer(element,'pointerup',290,180,extra);await tick();expect(document.body.textContent).toContain(`Выбрано: ${count}`);expect(frames.count()).toBe(0);expect(nestedHistory.past).toHaveLength(0)
})
it('can reverse a panning marquee, shrink its time range and stop at the left extent',async()=>{
 const frames=animationFrames(),element=await narrowMarquee({},160);await frames.step(62);expect(document.body.textContent).toContain('Выбрано: 3')
 pointer(element,'pointermove',10,180);await frames.step(62);expect(document.body.textContent).toContain('Выбрано: 2')
 await frames.step(150);expect(frames.count()).toBe(0);expect(document.querySelector('[aria-label="Видимый диапазон ключей"]')!.textContent).toContain('0.000–0.200');expect(document.body.textContent).toContain('Выбрано: 2')
 pointer(element,'pointerup',10,180);await tick();expect(document.body.textContent).toContain('Выбрано: 2');expect(nestedHistory.past).toHaveLength(0)
})
