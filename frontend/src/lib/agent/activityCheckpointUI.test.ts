// @ts-expect-error Tests need the Svelte client entry.
import {mount,tick,unmount} from '../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
// @ts-expect-error Exercise client onMount/onDestroy, rather than the SSR condition in Vitest.
vi.mock('svelte',()=>import('../../../node_modules/svelte/src/index-client.js'))
import AgentActivityPanel from './AgentActivityPanel.svelte'
import {executeAgentTool} from './runtime.svelte'
import {agentSession} from './session.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {closeNestedComposition} from '../state/nestedComposition.svelte'
it('renders live checkpoint availability/lock state and restores from the actual replica row',async()=>{
 closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests();agentSession.checkpoints=[]
 const target=document.createElement('div');document.body.append(target)
 const app=mount(AgentActivityPanel,{target})
 try{
  const point=executeAgentTool('user','create_checkpoint',{label:'Row point'}) as {id:string}
  const original=compositionState.document.canvas.width
  compositionState.document={...compositionState.document,canvas:{...compositionState.document.canvas,width:640}}
  await tick();await tick()
  const button=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Восстановить точку')!
  expect(button).toBeDefined();expect(button.disabled).toBe(false)
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick();await tick()
  expect(button.disabled).toBe(true)
  agentSession.lock=null;await tick();await tick();expect(button.disabled).toBe(false)
  button.click();await tick();await tick();await tick()
  expect(compositionState.document.canvas.width).toBe(original);expect(compositionState.history.past).toHaveLength(1)
  executeAgentTool('user','delete_checkpoint',{id:point.id});await tick();await tick()
  expect(button.textContent).toBe('Точка недоступна');expect(button.disabled).toBe(true)
  expect(target.textContent).toContain('Контрольная точка: Row point')
 }finally{await unmount(app);target.remove();closeNestedComposition()}
})
