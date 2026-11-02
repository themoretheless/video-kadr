// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Editor from './ProceduralTransitionControls.svelte'
import {TRANSITION_DEFINITIONS,defaultProceduralTransition,proceduralTransitionReason} from '$lib/composition/transitionCatalog'
it('authors every catalog parameter through typed DOM controls without losing siblings',async()=>{
 let checked=0
 for(const definition of TRANSITION_DEFINITIONS){
  const value=defaultProceduralTransition(definition.id);const onchange=vi.fn()
  const target=document.createElement('div');document.body.append(target)
  const component=mount(Editor,{target,props:{value,onchange}});await tick()
  expect(target.querySelectorAll('select[aria-label="Ядро перехода"] option')).toHaveLength(131)
  for(const parameter of definition.params){
   const initial=parameter.default
   const suffix='Color' in initial?' R':'Vec2' in initial?' x':''
   const input=target.querySelector(`[aria-label="Параметр перехода ${parameter.id}${suffix}"]`) as HTMLInputElement|HTMLSelectElement
   expect(input,`${definition.id}/${parameter.id}`).not.toBeNull()
   if('Bool' in initial){(input as HTMLInputElement).checked=!initial.Bool;input.dispatchEvent(new Event('change',{bubbles:true}))}
   else if('Choice' in initial){input.value=String((initial.Choice+1)%(input as HTMLSelectElement).options.length);input.dispatchEvent(new Event('change',{bubbles:true}))}
   else{input.value='Color' in initial?'0.25':'Vec2' in initial?'12':String(initial.Float);input.dispatchEvent(new Event('input',{bubbles:true}))}
   await tick();const result=onchange.mock.lastCall![0]
   expect(proceduralTransitionReason(result),`${definition.id}/${parameter.id}`).toBeNull()
   for(const sibling of definition.params.filter(p=>p.id!==parameter.id))expect(result.params[sibling.id]).toEqual(value.params![sibling.id])
   checked++
  }
  await unmount(component);target.remove()
 }
 expect(checked).toBeGreaterThan(200)
})

it('authors Blur Dissolve strength through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.blur-dissolve'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода strength"]')!
 expect(input.value).toBe('60');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.blur-dissolve',params:{strength:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Whip Pan speed through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.whip-pan'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода speed"]')!
 expect(input.value).toBe('80');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.whip-pan',params:{speed:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors both Zoom directions through the mounted select',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.zoom-transition'),onchange}});await tick()
 const input=target.querySelector<HTMLSelectElement>('[aria-label="Параметр перехода direction"]')!
 expect(input.value).toBe('0');expect([...input.options].map(o=>o.textContent)).toEqual(['In','Out'])
 for(const direction of [1,0]){input.value=String(direction);input.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.zoom-transition',params:{direction:{Choice:direction}}})}
 await unmount(component);target.remove()
})

it('authors Light Leak Flash strength through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.light-leak-flash'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода strength"]')!
 expect(input.value).toBe('80');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.light-leak-flash',params:{strength:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Light Leak warmth through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.light-leak-transition'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода warmth"]')!
 expect(input.value).toBe('70');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.light-leak-transition',params:{warmth:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Zoom Blur Transition strength through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.zoom-blur-transition'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода strength"]')!
 expect(input.value).toBe('70');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.zoom-blur-transition',params:{strength:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Film Burn heat through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.film-burn'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода heat"]')!
 expect(input.value).toBe('70');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.film-burn',params:{heat:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Glitch amount through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.glitch-transition'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода amount"]')!
 expect(input.value).toBe('70');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.glitch-transition',params:{amount:{Float:37}}})
 await unmount(component);target.remove()
})

it('authors Glitch Warp amount through the mounted control',async()=>{
 const onchange=vi.fn();const target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{value:defaultProceduralTransition('concat.glitch-warp'),onchange}});await tick()
 const input=target.querySelector<HTMLInputElement>('[aria-label="Параметр перехода amount"]')!
 expect(input.value).toBe('75');input.value='37';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(onchange.mock.lastCall![0]).toEqual({effect:'concat.glitch-warp',params:{amount:{Float:37}}})
 await unmount(component);target.remove()
})
