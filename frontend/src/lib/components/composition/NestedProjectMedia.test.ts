// @ts-expect-error Tests require the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, setNestedMediaPlaceholder, removeNestedMedia, fillNestedMedia, fillAllNestedMedia, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import Controls from './NestedProjectMedia.svelte'
vi.mock('$lib/api', async original => ({ ...await original<typeof api>(), setNativeProjectMediaPlaceholder: vi.fn(), removeNativeProjectMedia: vi.fn(), fillNativeProjectMedia: vi.fn(), inspectNativeTemplate:vi.fn() }))
const source = { id: 'source-a', kind: 'image' as const, durationTicks: 0, width: 16, height: 16, hasAudio: false }
function project(): api.NativeNestedProject {
  const content = { ...buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 24, backgroundColor: '#000000' })), sources: { [source.id]: source } }
  return { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, other: { content, instances: [] } } }
}
let component: ReturnType<typeof mount> | undefined
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); vi.resetAllMocks(); document.body.innerHTML = '' })
it('marks shared media once through Rust and restores the registry with one Undo/Redo without invalidating output', async () => {
  const initial = project(); nestedComposition.document = initial; nestedComposition.activeCompositionId = 'other'; nestedComposition.timeUs = 123; nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.setNativeProjectMediaPlaceholder).mockImplementation(async (p, id, placeholder) => p.projectMedia?.[id]?.placeholder === placeholder ? p : { ...p, projectMedia: { ...p.projectMedia, [id]: { source, name: 'Shared source', placeholder } } })
  component = mount(Controls, { target: document.body }); await tick()
  expect(document.querySelectorAll('input[type=checkbox]')).toHaveLength(1); expect(document.body.textContent).toContain('Таймлайнов: 2')
  let check = document.querySelector<HTMLInputElement>('input[type=checkbox]')!; check.checked = true; check.dispatchEvent(new Event('change', { bubbles: true })); await tick(); await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledExactlyOnceWith(initial, source.id, true); expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' }); expect(nestedComposition.activeCompositionId).toBe('other'); expect(nestedComposition.timeUs).toBe(123)
  expect(nestedComposition.document?.compositions).toEqual(initial.compositions)
  await setNestedMediaPlaceholder(source.id, true); expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition(); await tick(); expect(nestedComposition.document).toEqual(initial); expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  redoNestedComposition(); await tick(); check = document.querySelector<HTMLInputElement>('input[type=checkbox]')!; expect(check.checked).toBe(true)
})
it('retains media with no timeline placement and drops a late result after project close', async () => {
  const initial = project(); initial.projectMedia = { [source.id]: { source, name: 'Retained', placeholder: true } }; initial.compositions = { root: { ...initial.compositions.root, content: { ...initial.compositions.root.content, sources: {} } } }
  nestedComposition.document = initial; component = mount(Controls, { target: document.body }); await tick(); expect(document.body.textContent).toContain('Таймлайнов: 0')
  let resolve!: (p: api.NativeNestedProject) => void; vi.mocked(api.setNativeProjectMediaPlaceholder).mockImplementation(() => new Promise(done => { resolve = done }))
  const pending = setNestedMediaPlaceholder(source.id, false); await vi.waitFor(() => expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledTimes(1)); closeNestedComposition(); resolve({ ...initial, projectMedia: { [source.id]: { source, name: 'Retained' } } }); await pending; await tick()
  expect(nestedComposition.document).toBeNull(); expect(nestedHistory.past).toHaveLength(0); expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledTimes(1)
})

it('removes media project-wide through Rust in one history step and invalidates rendered outputs', async () => {
  const initial = project(); nestedComposition.document = initial; nestedComposition.activeCompositionId = 'other'; nestedComposition.timeUs = 123; nestedComposition.renderedOutputs = { root: 'old movie' }
  const removed = { ...initial, compositions: Object.fromEntries(Object.entries(initial.compositions).map(([id,node]) => [id,{...node,content:{...node.content,sources:{}}}])) }
  vi.mocked(api.removeNativeProjectMedia).mockResolvedValue(removed)
  component = mount(Controls,{target:document.body}); await tick()
  document.querySelector<HTMLButtonElement>('button')!.click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(api.removeNativeProjectMedia).toHaveBeenCalledExactlyOnceWith(initial,source.id)
  expect(nestedComposition.document).toEqual(removed); expect(nestedComposition.renderedOutputs).toEqual({})
  expect(nestedComposition.activeCompositionId).toBe('other'); expect(nestedComposition.timeUs).toBe(123)
  undoNestedComposition(); expect(nestedComposition.document).toEqual(initial); expect(nestedComposition.renderedOutputs).toEqual({root:'old movie'})
  redoNestedComposition(); expect(nestedComposition.document).toEqual(removed); expect(nestedComposition.renderedOutputs).toEqual({})
})
it('keeps the project and history on refusal and ignores a removal after close', async () => {
  const initial=project(); nestedComposition.document=initial
  vi.mocked(api.removeNativeProjectMedia).mockRejectedValue(new Error('dependent mask'))
  await removeNestedMedia(source.id)
  expect(nestedComposition.document).toEqual(initial); expect(nestedHistory.past).toHaveLength(0); expect(nestedHistory.error).toBe('dependent mask')
  let resolve!:(value:api.NativeNestedProject)=>void
  vi.mocked(api.removeNativeProjectMedia).mockImplementation(()=>new Promise(done=>{resolve=done}))
  const pending=removeNestedMedia(source.id); await vi.waitFor(()=>expect(api.removeNativeProjectMedia).toHaveBeenCalledTimes(2))
  closeNestedComposition(); resolve(initial); await pending
  expect(nestedComposition.document).toBeNull(); expect(nestedHistory.past).toHaveLength(0)
})

it('fills a project slot through the registered-source route with one Undo/Redo and drops stale output',async()=>{
 const initial=project();initial.projectMedia={[source.id]:{source,name:'Slot',placeholder:true}};nestedComposition.document=initial;nestedComposition.renderedOutputs={root:'old movie'}
 const filled={...initial,projectMedia:{[source.id]:{source,librarySourceId:'replacement',name:'Fresh'}}}
 vi.mocked(api.fillNativeProjectMedia).mockResolvedValue(filled)
 component=mount(Controls,{target:document.body,props:{token:'session',sources:[{label:'New file',source:{...source,id:'replacement'}}]}});await tick()
 const select=document.querySelector<HTMLSelectElement>('select')!;select.value='replacement';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="Заполнить слот проекта: Slot"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(api.fillNativeProjectMedia).toHaveBeenCalledExactlyOnceWith(initial,source.id,'replacement','session');expect(nestedComposition.document).toEqual(filled);expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(initial);expect(nestedComposition.renderedOutputs).toEqual({root:'old movie'});redoNestedComposition();expect(nestedComposition.document).toEqual(filled)
})
it('keeps native FillSlot atomic on refusal, context changes and close',async()=>{
 const initial=project();nestedComposition.document=initial;vi.mocked(api.fillNativeProjectMedia).mockRejectedValue(new Error('retained controls'))
 await fillNestedMedia(source.id,'replacement','session');expect(nestedComposition.document).toEqual(initial);expect(nestedHistory.past).toHaveLength(0);expect(nestedHistory.error).toBe('retained controls')
 let resolve!:(value:api.NativeNestedProject)=>void;vi.mocked(api.fillNativeProjectMedia).mockImplementation(()=>new Promise(done=>{resolve=done}))
 let current=true;const pending=fillNestedMedia(source.id,'replacement','session',()=>{if(!current)throw new Error('source context changed')});await vi.waitFor(()=>expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(2));current=false;resolve({...initial,projectMedia:{}});await pending;expect(nestedComposition.document).toEqual(initial);expect(nestedHistory.past).toHaveLength(0)
 const late=fillNestedMedia(source.id,'replacement','session');await vi.waitFor(()=>expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(3));closeNestedComposition();resolve(initial);await late;expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

function template():api.NativeNestedProject{
 const initial=project()
 initial.projectMedia={one:{source:{...source,id:'one'},name:'One',placeholder:true},two:{source:{...source,id:'two'},name:'Two',placeholder:true}}
 return initial
}
it('fills all template slots off-document and commits one Undo/Redo through the mounted control',async()=>{
 const initial=template();nestedComposition.document=initial;nestedComposition.renderedOutputs={root:'old movie'}
 const inputs:api.NativeNestedProject[]=[]
 vi.mocked(api.fillNativeProjectMedia).mockImplementation(async(p,id,replacement)=>{
  inputs.push(p);expect(nestedComposition.document).toEqual(initial);expect(nestedHistory.past).toHaveLength(0)
  return {...p,projectMedia:{...p.projectMedia,[id]:{...p.projectMedia![id]!,placeholder:false,librarySourceId:replacement}}}
 })
 component=mount(Controls,{target:document.body,props:{token:'session',sources:[{label:'New',source:{...source,id:'replacement'}}]}});await tick()
 const button=[...document.querySelectorAll('button')].find(b=>b.textContent==='Заполнить все слоты шаблона')!
 expect(button.disabled).toBe(true)
 for(const select of document.querySelectorAll('select')){select.value='replacement';select.dispatchEvent(new Event('change',{bubbles:true}))}
 await tick();expect(button.disabled).toBe(false);button.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(2);expect(inputs[1]!.projectMedia!.one!.placeholder).toBe(false)
 const filled=nestedComposition.document;expect(Object.values(filled!.projectMedia!).every(item=>item.placeholder===false)).toBe(true)
 expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(initial);expect(nestedComposition.renderedOutputs).toEqual({root:'old movie'})
 redoNestedComposition();expect(nestedComposition.document).toEqual(filled)
})
it('does not publish a partial template when a later slot refuses or a choice is missing',async()=>{
 const initial=template();nestedComposition.document=initial;nestedComposition.renderedOutputs={root:'old movie'}
 await fillAllNestedMedia({one:'replacement'},'session')
 expect(api.fillNativeProjectMedia).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('каждого слота')
 vi.mocked(api.fillNativeProjectMedia).mockImplementation(async(p,id)=>{
  if(id==='two')throw new Error('derived binding needs reanalysis')
  return {...p,projectMedia:{...p.projectMedia,one:{...p.projectMedia!.one!,placeholder:false}}}
 })
 await fillAllNestedMedia({one:'replacement',two:'replacement'},'session')
 expect(nestedComposition.document).toEqual(initial);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old movie'})
 expect(nestedHistory.error).toBe('derived binding needs reanalysis')
})
it('drops staged template fills on context change and project close',async()=>{
 const initial=template();nestedComposition.document=initial
 let resolve!:(value:api.NativeNestedProject)=>void
 vi.mocked(api.fillNativeProjectMedia).mockImplementation(()=>new Promise(done=>{resolve=done}))
 let current=true
 const pending=fillAllNestedMedia({one:'replacement',two:'replacement'},'session',()=>{if(!current)throw new Error('context changed')})
 await vi.waitFor(()=>expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(1));current=false;resolve(initial);await pending
 expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(1);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(initial)
 const late=fillAllNestedMedia({one:'replacement',two:'replacement'},'session')
 await vi.waitFor(()=>expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(2));closeNestedComposition();resolve(initial)
 await late;expect(api.fillNativeProjectMedia).toHaveBeenCalledTimes(2)
 expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

it('shows native slot order and durations for the active timeline and cancels stale inspections',async()=>{
 const initial=template();nestedComposition.document=initial;nestedComposition.activeCompositionId='other'
 let resolve!:(value:api.NativeTemplateInfo)=>void
 vi.mocked(api.inspectNativeTemplate).mockImplementation(()=>new Promise(done=>{resolve=done}))
 component=mount(Controls,{target:document.body});await tick()
 expect(api.inspectNativeTemplate).not.toHaveBeenCalled()
 const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 await vi.waitFor(()=>expect(api.inspectNativeTemplate).toHaveBeenCalledTimes(1))
 const signal=vi.mocked(api.inspectNativeTemplate).mock.calls[0]![2]!
 expect(vi.mocked(api.inspectNativeTemplate).mock.calls[0]![1]).toBe('other')
 resolve({schemaVersion:1,compositionId:'other',width:16,height:16,slots:[{mediaId:'two',name:'Two',kind:'image',seconds:5.5},{mediaId:'one',name:'One',kind:'image',seconds:2}]})
 await vi.waitFor(()=>expect(document.body.textContent).toContain('5,5 с'))
 expect([...document.querySelectorAll('input[type=checkbox]')].map(input=>input.getAttribute('aria-label')).slice(0,2)).toEqual(['Слот шаблона: Two','Слот шаблона: One'])
 nestedComposition.activeCompositionId='root';await tick();await vi.waitFor(()=>expect(api.inspectNativeTemplate).toHaveBeenCalledTimes(2));expect(signal.aborted).toBe(true)
 details.open=false;details.dispatchEvent(new Event('toggle'));await tick()
 const lateSignal=vi.mocked(api.inspectNativeTemplate).mock.calls[1]![2]!;expect(lateSignal.aborted).toBe(true)
 resolve({schemaVersion:1,compositionId:'root',width:16,height:16,slots:[{mediaId:'one',name:'One',kind:'image',seconds:999},{mediaId:'two',name:'Two',kind:'image',seconds:0}]})
 await tick();expect(document.body.textContent).not.toContain('999 с')
})
