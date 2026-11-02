// @ts-expect-error Client mount required under Vitest SSR conditions.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import Editor from './GraphicLayerAnimation.svelte'
import type { GraphicContent } from '$lib/composition/types'
import { graphicShapeChannels,graphicTextChannels } from '$lib/composition/graphicLayerProperties'
for(const kind of ['shape','text']as const)it(`authors every ${kind} channel at retained local time`,async()=>{
 const target=document.createElement('div');document.body.append(target)
 const content:GraphicContent={kind,parameters:{}}
 const onchange=vi.fn();const component=mount(Editor,{target,props:{content,label:'layer',localUs:750000,width:80,height:64,onchange}});await tick()
 for(const[channel]of kind==='shape'?graphicShapeChannels:graphicTextChannels){
  const select=target.querySelector<HTMLSelectElement>('[aria-label="layer animation channel"]')!;select.value=channel;select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  target.querySelector<HTMLButtonElement>('[aria-label="layer add key"]')!.click()
  expect(onchange.mock.lastCall![0].animation[channel], `${channel} select=${select.value} actual=${JSON.stringify(onchange.mock.lastCall![0].animation)}`).toMatchObject({track:{timeBase:1000000,keyframes:[{tick:750000}]}})
 }
 if(kind==='text'){target.querySelector<HTMLButtonElement>('[aria-label="layer add text key"]')!.click();expect(onchange.mock.lastCall![0]).toMatchObject({sourceTextAnimation:{keyframes:[{tick:750000,value:'Текст'}]}})}
 await unmount(component);target.remove()
})
it('edits signed key times, elastic parameters and source-text keys in the authored clock',async()=>{
 const target=document.createElement('div');document.body.append(target)
 const content:GraphicContent={kind:'text',parameters:{text:'Привет'},animation:{opacity:{mode:'keyframes',track:{timeBase:3,interpolation:'linear',keyframes:[{tick:-1,value:20,interpolation:{kind:'elastic',dir:'out',amplitude:2,period:.4}}]}}},sourceTextAnimation:{timeBase:3,keyframes:[{tick:-1,value:'Привет'}]}}
 const onchange=vi.fn();const component=mount(Editor,{target,props:{content,label:'layer',localUs:500000,width:80,height:64,onchange}});await tick()
 const edit=(label:string,value:string)=>{const input=target.querySelector<HTMLInputElement|HTMLTextAreaElement>(`[aria-label="${label}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))}
 edit('layer opacity key 0 tick','-2');expect(onchange.mock.lastCall![0]).toMatchObject({animation:{opacity:{track:{timeBase:3,keyframes:[{tick:-2}]}}}})
 edit('layer opacity key 0 период','.8');expect(onchange.mock.lastCall![0]).toMatchObject({animation:{opacity:{track:{keyframes:[{interpolation:{kind:'elastic',period:.8,amplitude:2}}]}}}})
 for(const side of ['before','after'])for(const mode of ['hold','continue','loop','ping_pong','offset']){edit(`layer extrapolation ${side}`,mode);expect(onchange.mock.lastCall![0]).toMatchObject({animation:{opacity:{track:{timeBase:3,extrapolation:{[side]:mode}}}}})}
 edit('layer text key 0 tick','4');expect(onchange.mock.lastCall![0]).toMatchObject({sourceTextAnimation:{timeBase:3,keyframes:[{tick:4}]}})
 edit('layer text key 0 value','🙂');expect(onchange.mock.lastCall![0]).toMatchObject({sourceTextAnimation:{keyframes:[{value:'🙂'}]}})
 await unmount(component);target.remove()
})
