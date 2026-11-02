// @ts-expect-error Client mount required under Vitest SSR conditions.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import Editor from './GraphicLayerPins.svelte'
import { captureGraphicPin } from '$lib/api'
vi.mock('$lib/api', () => ({captureGraphicPin:vi.fn()}))
import type { GraphicLayer } from '$lib/composition/types'
it('edits targets, all edges and fractional signed offsets without changing other pin fields', async () => {
  const layer: GraphicLayer = { uid: 1, enabled: true, content: { kind: 'shape', parameters: {} }, pin: { to: {layer: 2}, left: true, top: false, right: true, bottom: false, offsets: [-2.5, 3, 4, 5] } }
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn()
  const component = mount(Editor, {target, props: {layer, layers: [layer, {...layer, uid: 2, enabled: false}], onchange}}); await tick()
  const select = target.querySelector('select')!
  expect([...select.options].map(option => option.value)).toEqual(['frame', '2'])
  expect(select.value).toBe('2')
  select.value = 'frame'; select.dispatchEvent(new Event('change', {bubbles:true}))
  expect(onchange.mock.lastCall![0]).toEqual({...layer.pin, to:'frame'})
  for (const [index, edge] of ['left','top','right','bottom'].entries()) {
    const input = target.querySelector<HTMLInputElement>(`[aria-label="Слой 1 отступ ${edge}"]`)!
    input.value = '-7.125'; input.dispatchEvent(new Event('change', {bubbles:true}))
    const offsets = [...layer.pin!.offsets]; offsets[index] = -7.125
    expect(onchange.mock.lastCall![0]).toEqual({...layer.pin, offsets})
    const checkbox = target.querySelector<HTMLInputElement>(`[aria-label="Слой 1 привязка ${edge}"]`)!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', {bubbles:true}))
    expect(onchange.mock.lastCall![0]).toEqual({...layer.pin, [edge]:true})
  }
  [...target.querySelectorAll('button')].find(button => button.textContent?.includes('Удалить'))!.click(); expect(onchange.mock.lastCall![0]).toBeUndefined()
  expect(layer.pin!.offsets).toEqual([-2.5,3,4,5])
  await unmount(component); target.remove()
})

it('captures through the native API using retained local time and commits its distances', async () => {
  const layer: GraphicLayer = {uid:1,enabled:true,content:{kind:'text',parameters:{text:'Привет'}},pin:{to:'frame',left:true,top:false,right:true,bottom:false,offsets:[0,0,0,0]}}
  const pin = {...layer.pin!,offsets:[1.25,2.5,-3.75,4] as const}
  vi.mocked(captureGraphicPin).mockResolvedValueOnce(pin)
  const target=document.createElement('div');document.body.append(target)
  const onchange=vi.fn();const component=mount(Editor,{target,props:{layer,layers:[layer],width:80,height:64,localUs:750000,onchange}});await tick()
  ;[...target.querySelectorAll('button')].find(button=>button.textContent?.includes('Захватить'))!.click()
  await vi.waitFor(()=>expect(onchange).toHaveBeenCalledWith(pin))
  expect(captureGraphicPin).toHaveBeenCalledWith({layers:[layer]},80,64,750000,1,'frame',[true,false,true,false])
  await unmount(component);target.remove()
})
