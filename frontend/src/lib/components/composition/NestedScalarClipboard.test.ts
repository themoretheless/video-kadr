import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {nestedScalarClipboard} from '$lib/state/nestedScalarClipboard.svelte'
import {nestedEffectPreview} from '$lib/state/nestedEffectPreview.svelte'
import {authState} from '$lib/state/auth.svelte'
import {nativeScalarChannels} from '$lib/composition/nativeScalarTimeline'
import {captureNativeScalarClipboard,serializeNativeScalarClipboard} from '$lib/composition/nativeScalarClipboard'
import {scalarClipboardProject} from '$lib/composition/fixtures/nativeScalarClipboardProject'
import Timeline from './NestedScalarTimeline.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const readText=vi.fn<()=>Promise<string>>(),writeText=vi.fn<(text:string)=>Promise<void>>()
const previous=Object.getOwnPropertyDescriptor(navigator,'clipboard')
beforeEach(()=>{
 closeNestedComposition();nestedScalarClipboard.value=null;nestedEffectPreview.value=null;authState.token='clipboard-session'
 vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});readText.mockReset();writeText.mockReset();writeText.mockResolvedValue()
 Object.defineProperty(navigator,'clipboard',{configurable:true,value:{readText,writeText}})
 nestedComposition.document=scalarClipboardProject();nestedComposition.activeCompositionId='root';nestedComposition.projectId='clipboard-project';nestedComposition.revision=3;nestedComposition.timeUs=2250000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();nestedScalarClipboard.value=null;nestedEffectPreview.value=null;authState.token=null;document.body.innerHTML='';if(previous)Object.defineProperty(navigator,'clipboard',previous);else Reflect.deleteProperty(navigator,'clipboard')})
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(item=>item.textContent===text)!
async function open(){component=mount(Timeline,{target:document.body});await tick();button('Выбрать все ключи').click();await tick()}
async function copy(){button('Копировать выбранные ключи').click();await vi.waitFor(()=>expect(nestedScalarClipboard.value).not.toBeNull());await tick()}
async function target(){const menu=document.querySelector<HTMLSelectElement>('[aria-label="Клип для вставки скалярных ключей"]')!;menu.value='target';menu.dispatchEvent(new Event('change',{bubbles:true}));nestedComposition.timeUs=2250000;await tick()}
function packet(){const project=nestedComposition.document!;return captureNativeScalarClipboard(project,'root',nativeScalarChannels(project,'root').flatMap(channel=>channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick}))))}
it('copies without mutation and pastes the reviewed snapshot into constant channels as one Undo/Redo',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();await copy();const captured=serializeNativeScalarClipboard(nestedScalarClipboard.value!)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
 await target();button('Предпросмотр вставки ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull();const preview=JSON.stringify(nestedEffectPreview.value!.project)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 button('Вставить скалярные ключи').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nestedComposition.renderedOutputs).toEqual({});expect(evaluateNestedFrame).toHaveBeenCalledOnce();expect(nestedEffectPreview.value).toBeNull();expect(document.body.textContent).toContain('Выбрано: 6')
 expect(serializeNativeScalarClipboard(nestedScalarClipboard.value!)).toBe(captured)
 const oracle=process.env.NATIVE_SCALAR_TIMELINE_PASTE_ORACLE
 if(oracle){nestedComposition.timeUs=2750000;writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))}
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'});redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it('cuts atomically and leaves clipboard controls available after all animation lanes become constant',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();button('Вырезать выбранные ключи').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(nestedScalarClipboard.value?.groups).toHaveLength(2);expect(nativeScalarChannels(nestedComposition.document!,'root')).toEqual([]);expect(button('Предпросмотр вставки ключей')).toBeTruthy()
 const captured=serializeNativeScalarClipboard(nestedScalarClipboard.value!);undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(serializeNativeScalarClipboard(nestedScalarClipboard.value!)).toBe(captured)
})
it.each(['refusal','replacement','cancel'] as const)('preserves the prior clipboard and document on cut %s',async kind=>{
 const original=packet();nestedScalarClipboard.value=original
 let finish!:(value:{entries:[]})=>void
 if(kind==='refusal')vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Native cut refused'))
 else vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();button('Вырезать выбранные ключи').click()
 let expected=original
 if(kind!=='refusal'){
  await vi.waitFor(()=>expect(finish).toBeDefined())
  if(kind==='replacement'){expected=JSON.parse(JSON.stringify(original));nestedScalarClipboard.value=expected}else button('Отменить операцию буфера ключей').click()
  await tick();finish({entries:[]})
 }
 await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedScalarClipboard.value).toBe(expected);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
})
it.each(['clipboard','cursor','target','session','foreign'] as const)('discards a prepared paste on %s change',async kind=>{
 await open();await copy();await target();button('Предпросмотр вставки ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull()
 let foreign:typeof nestedEffectPreview.value=null
 if(kind==='clipboard')nestedScalarClipboard.value=JSON.parse(JSON.stringify(nestedScalarClipboard.value))
 if(kind==='cursor')nestedComposition.timeUs++
 if(kind==='target'){const menu=document.querySelector<HTMLSelectElement>('[aria-label="Клип для вставки скалярных ключей"]')!;menu.value='source';menu.dispatchEvent(new Event('change',{bubbles:true}))}
 if(kind==='session')authState.token='other-session'
 if(kind==='foreign'){foreign={...nestedEffectPreview.value!,owner:Symbol('foreign preview'),gesture:{}};nestedEffectPreview.value=foreign}
 const expected=JSON.stringify(nestedComposition.document);await tick();expect(nestedEffectPreview.value).toBe(foreign);expect(button('Вставить скалярные ключи').disabled).toBe(true);expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(expected)
})
it('rejects stale paste admission before reactive cleanup and restores the original preview on native refusal',async()=>{
 await open();await copy();await target();button('Предпросмотр вставки ключей').click();await tick()
 nestedComposition.timeUs++;button('Вставить скалярные ключи').click();await tick();expect(evaluateNestedFrame).not.toHaveBeenCalled()
 nestedComposition.timeUs=2250000;await tick();button('Предпросмотр вставки ключей').click();await tick();const before=JSON.stringify(nestedComposition.document)
 vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Native paste refused'));button('Вставить скалярные ключи').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Native paste refused'));await tick()
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
})
it('reads and writes the strict system clipboard format without changing the document or history',async()=>{
 await open();await copy();const text=serializeNativeScalarClipboard(nestedScalarClipboard.value!),before=JSON.stringify(nestedComposition.document)
 button('Записать ключи в системный буфер').click();await vi.waitFor(()=>expect(writeText).toHaveBeenCalledWith(text));await tick()
 nestedScalarClipboard.value=null;readText.mockResolvedValue(text);button('Прочитать ключи из системного буфера').click();await vi.waitFor(()=>expect(nestedScalarClipboard.value).not.toBeNull());await tick()
 expect(serializeNativeScalarClipboard(nestedScalarClipboard.value!)).toBe(text);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it.each(['malformed','denied','late'] as const)('preserves a newer local clipboard on system read %s',async kind=>{
 await open();await copy();const original=nestedScalarClipboard.value!,text=serializeNativeScalarClipboard(original)
 let finish!:(value:string)=>void
 if(kind==='malformed')readText.mockResolvedValue('{"format":"other"}')
 if(kind==='denied')readText.mockRejectedValue(new Error('Clipboard permission denied'))
 if(kind==='late')readText.mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 button('Прочитать ключи из системного буфера').click();await vi.waitFor(()=>expect(readText).toHaveBeenCalled())
 let expected=original
 if(kind==='late'){expected=JSON.parse(text);nestedScalarClipboard.value=expected;await tick();finish(text)}
 await vi.waitFor(()=>expect(button('Прочитать ключи из системного буфера').disabled).toBe(false));await tick()
 expect(nestedScalarClipboard.value).toBe(expected);expect(nestedHistory.past).toHaveLength(0)
})
it('supports scoped keyboard copy/paste without intercepting text-field clipboard shortcuts',async()=>{
 await open();const key=document.querySelector<HTMLButtonElement>('[aria-label="source x ключ 250"]')!;key.focus();key.dispatchEvent(new KeyboardEvent('keydown',{key:'c',ctrlKey:true,bubbles:true}));await vi.waitFor(()=>expect(nestedScalarClipboard.value).not.toBeNull());await tick();await target()
 key.dispatchEvent(new KeyboardEvent('keydown',{key:'v',metaKey:true,bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const copy=serializeNativeScalarClipboard(nestedScalarClipboard.value!),input=document.querySelector<HTMLInputElement>('[aria-label="Масштаб времени нативных ключей"]')!,event=new KeyboardEvent('keydown',{key:'x',ctrlKey:true,bubbles:true,cancelable:true});input.dispatchEvent(event);await tick()
 expect(event.defaultPrevented).toBe(false);expect(serializeNativeScalarClipboard(nestedScalarClipboard.value!)).toBe(copy);expect(nestedHistory.past).toHaveLength(1)
})

it('cancels a paste during native validation without changing document, cache or clipboard',async()=>{
 let finish!:(value:{entries:[]})=>void
 vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();await copy();const captured=nestedScalarClipboard.value;await target();button('Предпросмотр вставки ключей').click();await tick();button('Вставить скалярные ключи').click();await vi.waitFor(()=>expect(finish).toBeDefined())
 button('Отменить операцию буфера ключей').click();await tick();expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![2]!.aborted).toBe(true);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'});expect(nestedScalarClipboard.value).toBe(captured);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull()
})
it.each(['session','project','unmount'] as const)('discards a delayed system clipboard read after %s changes',async kind=>{
 await open();await copy();const captured=nestedScalarClipboard.value,text=serializeNativeScalarClipboard(captured!)
 let finish!:(text:string)=>void
 readText.mockImplementation(()=>new Promise(resolve=>{finish=resolve}));button('Прочитать ключи из системного буфера').click();await vi.waitFor(()=>expect(finish).toBeDefined())
 if(kind==='session')authState.token='other-session'
 if(kind==='project')nestedComposition.document=JSON.parse(JSON.stringify(nestedComposition.document))
 if(kind==='unmount'){await unmount(component!);component=undefined}
 await tick();finish(text);await tick();expect(nestedScalarClipboard.value).toBe(captured);expect(nestedHistory.past).toHaveLength(0)
})
