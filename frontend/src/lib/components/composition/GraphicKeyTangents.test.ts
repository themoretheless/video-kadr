// @ts-expect-error Client mount required under Vitest SSR conditions.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {expect,it,vi} from 'vitest'
import Editor from './GraphicKeyTangents.svelte'
import type {CompositionAnimatableValue} from '$lib/composition/types'
it('writes native-compatible tangent modes, continuity and coordinates without altering the input envelope',async()=>{
 const animation:CompositionAnimatableValue={mode:'keyframes',track:{timeBase:3,interpolation:'linear',extrapolation:{before:'hold',after:'loop'},keyframes:[{tick:0,value:0},{tick:2,value:10},{tick:5,value:40}]}}
 const before=JSON.stringify(animation),onchange=vi.fn(),target=document.createElement('div');document.body.append(target)
 const component=mount(Editor,{target,props:{animation,tick:2,label:'key',onchange}});await tick()
 const edit=(label:string,value:string)=>{const node=target.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="${label}"]`)!;node.value=value;node.dispatchEvent(new Event('change',{bubbles:true}))}
 edit('key tangent incoming x','.4')
 expect(onchange.mock.lastCall![0]).toMatchObject({track:{timeBase:3,extrapolation:{after:'loop'},keyframes:[{interpolation:{kind:'bezier'}},{tick:2,tangents:{incoming:{x:.4,mode:'free'}}},{tick:5}]}})
 edit('key tangent mode','auto');expect(onchange.mock.lastCall![0]).toMatchObject({track:{keyframes:[{},{tangents:{incoming:{mode:'auto'},outgoing:{mode:'auto'},continuity:'smooth'}},{}]}})
 edit('key tangent continuity','smooth');expect(onchange.mock.lastCall![0]).toMatchObject({track:{keyframes:[{},{tangents:{continuity:'smooth'}},{}]}})
 onchange.mockClear();edit('key tangent outgoing x','1.5');expect(onchange).not.toHaveBeenCalled();await tick();expect(target.querySelector('[role="alert"]')?.textContent).toContain('Invalid')
 expect(JSON.stringify(animation)).toBe(before)
 await unmount(component);target.remove()
})
