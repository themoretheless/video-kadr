// @ts-expect-error Tests need the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import NestedMarkers from './NestedMarkers.svelte'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {agentSession} from '$lib/agent/session.svelte'
import {createComposition} from '$lib/composition/commands'
import {buildCompositionWireDocument} from '$lib/composition/payload'
vi.mock('$lib/api',async original=>({...await original<typeof api>(),evaluateNestedFrame:vi.fn(async()=>({}))}))
it('adds, navigates, edits and deletes native markers through actual controls and Undo',async()=>{
 closeNestedComposition();agentSession.lock=null
 const content=buildCompositionWireDocument(createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},child:{content:structuredClone(content),instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.timeUs=750000
 const target=document.createElement('div');document.body.append(target)
 const app=mount(NestedMarkers,{target})
 const button=(text:string)=>[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent===text)!
 try{
  await tick();button('Добавить маркер').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.metadata?.markers).toHaveLength(1))
  expect(nestedHistory.past).toHaveLength(1)
  const marker=nestedComposition.document!.compositions.root!.content.metadata!.markers![0]!
  expect(marker.tick).toBe(750000)
  nestedComposition.timeUs=0;await tick();button('Следующий маркер').click();expect(nestedComposition.timeUs).toBe(750000);expect(nestedHistory.past).toHaveLength(1)
  const name=target.querySelector<HTMLInputElement>(`[aria-label="Название маркера ${marker.id}"]`)!
  name.value='Renamed';name.dispatchEvent(new Event('change',{bubbles:true}))
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.metadata!.markers![0]!.label).toBe('Renamed'))
  await tick();button('Удалить маркер Renamed').click()
  await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.metadata?.markers).toHaveLength(0))
  undoNestedComposition();await tick();expect(nestedComposition.document!.compositions.root!.content.metadata?.markers).toHaveLength(1)
  nestedComposition.activeCompositionId='child';await tick();expect(target.textContent).toContain('Нет маркеров')
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick()
  expect(button('Добавить маркер').closest('fieldset')!.disabled).toBe(true)
 }finally{await unmount(app);target.remove();agentSession.lock=null;closeNestedComposition();vi.clearAllMocks()}
})
it('refuses a marker commit if a history lock appears while native validation is pending',async()=>{
 closeNestedComposition();agentSession.lock=null
 const content=buildCompositionWireDocument(createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}};nestedComposition.activeCompositionId='root'
 const before=JSON.stringify(nestedComposition.document)
 let release!:()=>void
 const pending=new Promise<void>(resolve=>{release=resolve})
 vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(async()=>{await pending;return {} as Awaited<ReturnType<typeof api.evaluateNestedFrame>>})
 const target=document.createElement('div');document.body.append(target)
 const app=mount(NestedMarkers,{target})
 try{
  await tick();[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Добавить маркер')!.click()
  await vi.waitFor(()=>expect(nestedHistory.busy).toBe(true))
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000}
  release()
  await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false))
  expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedHistory.error).toContain('Hold')
 }finally{release();await unmount(app);target.remove();agentSession.lock=null;closeNestedComposition();vi.clearAllMocks()}
})
