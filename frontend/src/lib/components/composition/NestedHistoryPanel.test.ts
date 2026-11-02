// @ts-expect-error Tests need the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import NestedHistoryPanel from './NestedHistoryPanel.svelte'
import {closeNestedComposition,nestedComposition,nestedHistory,editNestedMetadata,jumpNestedHistory,adoptImportedConcatProject} from '$lib/state/nestedComposition.svelte'
import {describeHistoryStacks} from '$lib/agent/historyNavigation'
import {agentSession} from '$lib/agent/session.svelte'
import {createComposition} from '$lib/composition/commands'
import {buildCompositionWireDocument} from '$lib/composition/payload'
vi.mock('$lib/api',async original=>({...await original<typeof api>(),evaluateNestedFrame:vi.fn(async()=>({entries:[]}))}))
it('jumps through actual native history controls without adding entries and respects locks',async()=>{
 closeNestedComposition();agentSession.lock=null
 const content=buildCompositionWireDocument(createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}};nestedComposition.activeCompositionId='root'
 const initial=JSON.stringify(nestedComposition.document)
 for(const sequenceName of ['One','Two','Three'])await editNestedMetadata('root',metadata=>({...metadata,sequenceName}))
 const ids=describeHistoryStacks(nestedHistory.past,nestedHistory.future).entries.map(entry=>entry.id)
 const target=document.createElement('div');document.body.append(target)
 const app=mount(NestedHistoryPanel,{target})
 try{
  await tick();const details=target.querySelector('details')!;details.open=true
  target.querySelector<HTMLButtonElement>('[data-native-history-position="1"]')!.click();await tick()
  expect(nestedComposition.document!.compositions.root!.content.metadata?.sequenceName).toBe('One')
  expect(nestedHistory.past.length+nestedHistory.future.length).toBe(3)
  target.querySelector<HTMLButtonElement>('[data-native-history-position="3"]')!.click();await tick()
  expect(nestedComposition.document!.compositions.root!.content.metadata?.sequenceName).toBe('Three')
  expect(describeHistoryStacks(nestedHistory.past,nestedHistory.future).entries.map(entry=>entry.id)).toEqual(ids)
  const baseline=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='До первого доступного изменения')!
  baseline.click();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(initial)
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick()
  expect(target.querySelector<HTMLButtonElement>('[data-native-history-position="3"]')!.disabled).toBe(true)
  expect(()=>jumpNestedHistory(ids[2]!)).toThrow('Hold')
 }finally{await unmount(app);target.remove();agentSession.lock=null;closeNestedComposition()}
})

it('keeps native history reachable when the base state closes an imported project',async()=>{
 closeNestedComposition();agentSession.lock=null
 const content=buildCompositionWireDocument(createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}))
 const project:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
 expect(await adoptImportedConcatProject(project,'Imported')).toBe(true)
 const target=document.createElement('div');document.body.append(target);const app=mount(NestedHistoryPanel,{target})
 try{
  await tick();[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='До первого доступного изменения')!.click();await tick()
  expect(nestedComposition.document).toBeNull();expect(target.textContent).toContain('История вложенной композиции')
  target.querySelector<HTMLButtonElement>('[data-native-history-position="1"]')!.click();await tick()
  expect(nestedComposition.document).toEqual(project);expect(nestedHistory.past.length+nestedHistory.future.length).toBe(1)
 }finally{await unmount(app);target.remove();closeNestedComposition()}
})
