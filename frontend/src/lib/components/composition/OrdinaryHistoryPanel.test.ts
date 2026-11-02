// @ts-expect-error Tests need the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it} from 'vitest'
import OrdinaryHistoryPanel from './OrdinaryHistoryPanel.svelte'
import {compositionState,compositionRenderOutput,commitDocument,resetCompositionForTests,setCompositionStorageForTests} from '$lib/state/composition/core.svelte'
import {jumpCompositionHistory} from '$lib/state/composition/transport.svelte'
import {createComposition} from '$lib/composition/commands'
import {describeHistoryStacks} from '$lib/agent/historyNavigation'
import {agentSession} from '$lib/agent/session.svelte'
it('navigates ordinary history while restoring media bindings and export output as well as the document',async()=>{
 setCompositionStorageForTests(null);resetCompositionForTests();agentSession.lock=null
 const initial=JSON.stringify(compositionState.document)
 for(const [index,name] of ['One','Two','Three'].entries())commitDocument(createComposition({width:16*(index+1),height:16,fps:10,backgroundColor:'#000000'}),{asset:{id:'asset',url:`blob:owned-${name}`,filename:`${name}.wav`,mediaType:'audio',duration:5,width:0,height:0}},{...compositionRenderOutput(),videoBitrateKbps:1000*(index+1)})
 const history=describeHistoryStacks(compositionState.history.past,compositionState.history.future),ids=history.entries.map(entry=>entry.id)
 const target=document.createElement('div');document.body.append(target);const app=mount(OrdinaryHistoryPanel,{target})
 try{
  await tick();target.querySelector('details')!.open=true
  target.querySelector<HTMLButtonElement>('[data-history-position="1"]')!.click();await tick()
  expect(compositionState.document.canvas.width).toBe(16);expect(compositionState.media.asset?.filename).toBe('One.wav');expect(compositionState.export.videoBitrateKbps).toBe(1000)
  expect(compositionState.history.past.length+compositionState.history.future.length).toBe(3)
  target.querySelector<HTMLButtonElement>('[data-history-position="3"]')!.click();await tick()
  expect(compositionState.document.canvas.width).toBe(48);expect(compositionState.media.asset?.filename).toBe('Three.wav');expect(compositionState.export.videoBitrateKbps).toBe(3000)
  expect(describeHistoryStacks(compositionState.history.past,compositionState.history.future).entries.map(entry=>entry.id)).toEqual(ids)
  compositionState.save.busy=true;await tick();expect(target.querySelector<HTMLButtonElement>('[data-history-position="1"]')!.disabled).toBe(true);compositionState.save.busy=false
  agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000};await tick();expect(()=>jumpCompositionHistory(ids[0]!)).toThrow('Hold');agentSession.lock=null
  jumpCompositionHistory(ids[0]!,'before');await tick();expect(JSON.stringify(compositionState.document)).toBe(initial);expect(compositionState.media).toEqual({})
 }finally{await unmount(app);target.remove();agentSession.lock=null;resetCompositionForTests()}
})
