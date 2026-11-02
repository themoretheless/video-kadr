import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {nestedEffectPreview} from '$lib/state/nestedEffectPreview.svelte'
import {authState} from '$lib/state/auth.svelte'
import {agentSession} from '$lib/agent/session.svelte'
import {createComposition,addTrack,addClip} from '$lib/composition/commands'
import {buildCompositionWireDocument} from '$lib/composition/payload'
import {nativeScalarChannels} from '$lib/composition/nativeScalarTimeline'
import Timeline from './NestedScalarTimeline.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();nestedEffectPreview.value=null;authState.token='timeline-session';vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 let doc=createComposition({width:128,height:96,fps:4,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'text',kind:'text',name:'Text',locked:false,hidden:false,clips:[]});doc=addClip(doc,'text',{id:'title',kind:'text',text:'Keys',timelineStartTicks:1000000,durationTicks:2000000,x:0,y:0,opacity:1,style:{fontSizePx:24,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc),clip=content.tracks[0]!.clips[0]!;if(!('transform' in clip))throw new Error('Visual required')
 Object.assign(clip.transform,{x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:750,value:20},{tick:1250,value:40}]}}})
 Object.assign(clip,{opacity:{mode:'keyframes',track:{timeBase:24,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:12,value:0.5},{tick:24,value:1}]}}})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}};nestedComposition.activeCompositionId='root';nestedComposition.projectId='timeline-project';nestedComposition.revision=3;nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();nestedEffectPreview.value=null;authState.token=null;document.body.innerHTML=''})
function key(property:string,tick:number){return document.querySelector<HTMLButtonElement>(`[aria-label="title ${property} ключ ${tick}"]`)!}
function button(text:string){return [...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>element.textContent===text)!}
async function open(){component=mount(Timeline,{target:document.body});await tick();for(const ruler of document.querySelectorAll<HTMLElement>('.ruler'))ruler.getBoundingClientRect=()=>new DOMRect(0,0,300,32)}
function pointer(target:Element,type:string,x:number,id=1){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x});Object.defineProperty(event,'pointerId',{value:id});target.dispatchEvent(event)}
async function select(){key('x',250).click();key('x',750).dispatchEvent(new MouseEvent('click',{bubbles:true,ctrlKey:true}));key('x',1250).dispatchEvent(new MouseEvent('click',{bubbles:true,shiftKey:true}));key('opacity',0).dispatchEvent(new MouseEvent('click',{bubbles:true,metaKey:true}));await tick();nestedComposition.timeUs=1500000;await tick()}
it('moves a mixed-clock selection with native preview and one Undo/Redo, preserving the exact reviewed document',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();await select()
 expect(document.body.textContent).toContain('Выбрано: 4')
 const mark=key('x',250);pointer(mark,'pointerdown',100);pointer(mark,'pointermove',125);await tick()
 expect(nestedEffectPreview.value).not.toBeNull();const preview=JSON.stringify(nestedEffectPreview.value!.project)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 expect(Number.parseFloat(key('x',250).style.left)).toBeCloseTo(100*1250000/3000000)
 pointer(mark,'pointerup',125);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).toHaveBeenCalledOnce();expect(nestedComposition.renderedOutputs).toEqual({})
 expect(key('x',750).getAttribute('aria-pressed')).toBe('true');expect(document.activeElement).toBe(key('x',750))
 const channels=nativeScalarChannels(nestedComposition.document!,'root');expect(channels.find(channel=>channel.property==='x')!.value.track.sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
 expect(channels.find(channel=>channel.property==='opacity')!.value.track.keyframes.map(key=>key.tick)).toEqual([6,12,24])
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it.each(['escape','pointercancel','capture','clock','scope','session','project','in_place','lock','busy','unmount'] as const)('cancels an uncommitted key drag on %s',async kind=>{
 await open();await select();const mark=key('x',250);pointer(mark,'pointerdown',100);pointer(mark,'pointermove',125);await tick();expect(nestedEffectPreview.value).not.toBeNull()
 if(kind==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(kind==='pointercancel')pointer(mark,'pointercancel',125)
 if(kind==='capture')mark.dispatchEvent(new Event('lostpointercapture'))
 if(kind==='clock')nestedComposition.timeUs++
 if(kind==='scope')nestedComposition.activeCompositionId='other'
 if(kind==='session')authState.token='other-session'
 if(kind==='project')nestedComposition.document=JSON.parse(JSON.stringify(nestedComposition.document))
 if(kind==='in_place')Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{width:130})
 if(kind==='lock')agentSession.lock={owner:'agent',reason:'editing',expiresAt:Date.now()+60000}
 if(kind==='busy')nestedHistory.busy=true
 if(kind==='unmount'){await unmount(component!);component=undefined}
 const expected=JSON.stringify(nestedComposition.document);await tick();expect(nestedEffectPreview.value).toBeNull();pointer(mark,'pointerup',125);await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(expected);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('refuses occupied destinations atomically and leaves the original keys visible',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();key('x',750).click();await tick();const mark=key('x',750);pointer(mark,'pointerdown',100);pointer(mark,'pointermove',125);await tick()
 expect(document.body.textContent).toContain('уже есть другой ключ');expect(nestedEffectPreview.value).toBeNull();pointer(mark,'pointerup',125);await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('applies group presets and deletion each as one native transaction',async()=>{
 await open();button('Выбрать все ключи').click();await tick();expect(document.body.textContent).toContain('Выбрано: 6')
 const menu=document.querySelector<HTMLSelectElement>('[aria-label="Переход выбранных нативных ключей"]')!;menu.value='hold';menu.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(nativeScalarChannels(nestedComposition.document!,'root').every(channel=>channel.value.track.keyframes.every(key=>key.interpolation==='hold'))).toBe(true)
 button('Удалить выбранные ключи').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(nativeScalarChannels(nestedComposition.document!,'root')).toEqual([]);undoNestedComposition();await tick();expect(document.body.textContent).toContain('Дорожки ключей')
})
it('nudges selected groups by frames and adds a key on the original rational clock',async()=>{
 await open();button('Выбрать все ключи').click();await tick();key('x',250).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(nativeScalarChannels(nestedComposition.document!,'root').find(channel=>channel.property==='x')!.value.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1750])
 nestedComposition.timeUs=1625000;await tick();document.querySelector<HTMLButtonElement>('[aria-label="Добавить ключ title x по курсору"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
 expect(nativeScalarChannels(nestedComposition.document!,'root').find(channel=>channel.property==='x')!.value.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1500,1750])
})
it.each(['refusal','cancel'] as const)('does not commit after full-project validation %s',async kind=>{
 let finish!:(value:{entries:[]})=>void
 if(kind==='refusal')vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Native frame refused'))
 else vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();await select();const mark=key('x',250);pointer(mark,'pointerdown',100);pointer(mark,'pointermove',125);pointer(mark,'pointerup',125)
 if(kind==='cancel'){await vi.waitFor(()=>expect(finish).toBeDefined());window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}));await tick();expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![2]!.aborted).toBe(true);finish({entries:[]})}
 await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));await tick();expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 if(kind==='refusal')expect(document.body.textContent).toContain('Native frame refused')
})

it('previews time scaling around the first key and applies the reviewed mixed-clock result as one Undo',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();button('Выбрать все ключи').click();await tick()
 button('Предпросмотр масштаба ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull()
 const preview=JSON.stringify(nestedEffectPreview.value!.project)
 expect(document.body.textContent).toContain('Масштаб: 2.000000');expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
 button('Применить масштаб ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nestedEffectPreview.value).toBeNull();expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const channels=nativeScalarChannels(nestedComposition.document!,'root')
 expect(channels.find(channel=>channel.property==='x')!.value.track.keyframes.map(key=>key.tick)).toEqual([250,1250,2250]);expect(channels.find(channel=>channel.property==='opacity')!.value.track.keyframes.map(key=>key.tick)).toEqual([0,24,48])
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_SCALE_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it.each(['parameters','anchor','cancel','scope','time','foreign','in_place'] as const)('invalidates time scaling preview on %s without committing',async kind=>{
 await open();button('Выбрать все ключи').click();await tick();button('Предпросмотр масштаба ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull()
 let foreign:typeof nestedEffectPreview.value=null
 if(kind==='parameters'){const input=document.querySelector<HTMLInputElement>('[aria-label="Масштаб времени нативных ключей"]')!;input.value='1.5';input.dispatchEvent(new Event('input',{bubbles:true}))}
 if(kind==='anchor'){const menu=document.querySelector<HTMLSelectElement>('[aria-label="Опора масштаба нативных ключей"]')!;menu.value='cursor';menu.dispatchEvent(new Event('change',{bubbles:true}))}
 if(kind==='cancel')button('Отменить масштаб ключей').click()
 if(kind==='scope')nestedComposition.activeCompositionId='other'
 if(kind==='time')nestedComposition.timeUs++
 if(kind==='in_place')Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{width:130})
 if(kind==='foreign'){foreign={...nestedEffectPreview.value!,owner:Symbol('other preview'),gesture:{}};nestedEffectPreview.value=foreign}
 const expected=JSON.stringify(nestedComposition.document);await tick();expect(nestedEffectPreview.value).toBe(foreign);expect(JSON.stringify(nestedComposition.document)).toBe(expected);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('reports the applied wall-constrained factor and refuses zero scale',async()=>{
 await open();button('Выбрать все ключи').click();await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Масштаб времени нативных ключей"]')!;input.value='100';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Предпросмотр масштаба ключей').click();await tick()
 expect(document.body.textContent).toContain('Масштаб: 2.000000');button('Отменить масштаб ключей').click();input.value='0';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Предпросмотр масштаба ключей').click();await tick();expect(document.body.textContent).toContain('Некорректный масштаб')
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})
it('cancels time scale while complete-project validation is pending',async()=>{
 let finish!:(value:{entries:[]})=>void
 vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();button('Выбрать все ключи').click();await tick();button('Предпросмотр масштаба ключей').click();await tick();button('Применить масштаб ключей').click();await vi.waitFor(()=>expect(finish).toBeDefined())
 button('Отменить масштаб ключей').click();await tick();expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![2]!.aborted).toBe(true);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false))
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull()
})

it.each([['last',1250,24],['cursor',783,18]] as const)('scales around the chosen %s anchor',async(anchor,xTick,opacityTick)=>{
 Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{fpsMilli:30000})
 await open();button('Выбрать все ключи').click();await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Масштаб времени нативных ключей"]')!;input.value='0.5';input.dispatchEvent(new Event('input',{bubbles:true}))
 const menu=document.querySelector<HTMLSelectElement>('[aria-label="Опора масштаба нативных ключей"]')!;menu.value=anchor;menu.dispatchEvent(new Event('change',{bubbles:true}));await tick();button('Предпросмотр масштаба ключей').click();await tick()
 expect(nestedEffectPreview.value).not.toBeNull();const channels=nativeScalarChannels(nestedEffectPreview.value!.project,'root')
 expect(channels.find(channel=>channel.property==='x')!.value.track.keyframes[0]!.tick).toBe(xTick)
 expect(channels.find(channel=>channel.property==='opacity')!.value.track.keyframes.at(-1)!.tick).toBe(opacityTick)
 expect(nestedHistory.past).toHaveLength(0)
})

it('refuses Apply immediately after scale parameters change, before the invalidation effect runs',async()=>{
 await open();button('Выбрать все ключи').click();await tick();button('Предпросмотр масштаба ключей').click();await tick()
 const before=JSON.stringify(nestedComposition.document),input=document.querySelector<HTMLInputElement>('[aria-label="Масштаб времени нативных ключей"]')!
 input.value='1.5';input.dispatchEvent(new Event('input',{bubbles:true}));button('Применить масштаб ключей').click();await tick()
 expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
