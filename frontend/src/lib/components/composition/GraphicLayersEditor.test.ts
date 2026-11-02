// @ts-expect-error Client mount is required under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import Editor from './GraphicLayersEditor.svelte'
import { appendGraphicLayer } from '$lib/composition/graphicLayers'
let component: ReturnType<typeof mount> | undefined
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.innerHTML = '' })
it('edits visibility and paint order with stable IDs without mutating the original stack', async () => {
  const layers = appendGraphicLayer(appendGraphicLayer([], 'text'), 'shape')
  const onchange = vi.fn()
  component = mount(Editor, { target: document.body, props: { layers, onchange } })
  await tick()
  const input = document.querySelector<HTMLInputElement>('[aria-label="Видимость слоя 1"]')!
  input.checked = false; input.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange.mock.lastCall![0][0].enabled).toBe(false)
  expect(layers[0]!.enabled).toBe(true)
  document.querySelector<HTMLButtonElement>('[aria-label="Слой 1 вперёд"]')!.click()
  expect(onchange.mock.lastCall![0].map((layer: { uid: number }) => layer.uid)).toEqual([2, 1])
  document.querySelector<HTMLButtonElement>('[aria-label="Удалить слой 2"]')!.click()
  expect(onchange.mock.lastCall![0].map((layer: { uid: number }) => layer.uid)).toEqual([1])
})

it('uses native Unicode run adjustment when replacing formatted layer text', async () => {
  const api = await import('$lib/api')
  const adjusted = [{start:0,end:3,style:{underline:true}}]
  const native = vi.spyOn(api,'editGraphicTextRuns').mockResolvedValue(adjusted)
  const layers = [{uid:1,enabled:true,content:{kind:'text' as const,parameters:{text:'AБ',runs:[{start:0,end:1,style:{underline:true}}]}}}]
  const onchange = vi.fn()
  component=mount(Editor,{target:document.body,props:{layers,onchange}})
  await tick()
  const input=document.querySelector<HTMLTextAreaElement>('[aria-label="Текст слоя 1"]')!
  input.value='A🙂Б';input.dispatchEvent(new Event('change',{bubbles:true}))
  await vi.waitFor(()=>expect(onchange).toHaveBeenCalled())
  expect(native).toHaveBeenCalledWith('AБ',layers[0]!.content.parameters.runs,{kind:'adjust',newText:'A🙂Б'})
  expect(onchange.mock.lastCall![0][0].content.parameters).toMatchObject({text:'A🙂Б',runs:adjusted})
  expect(layers[0]!.content.parameters.text).toBe('AБ')
  native.mockRestore()
})

it('uses native protected signed time when authoring a source-text key', async () => {
  const api = await import('$lib/api')
  const native = vi.spyOn(api,'mapGraphicClock').mockResolvedValue(-500000)
  const layers = appendGraphicLayer([], 'text')
  const onchange = vi.fn()
  const responsiveTime={intro:1000000,outro:500000,designIn:-500000,designDuration:2000000}
  component=mount(Editor,{target:document.body,props:{layers,onchange,responsiveTime,relativeUs:0,durationUs:4000000,animationOffsetUs:250000,localUs:250000}})
  await tick()
  await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false))
  await tick()
  expect(document.querySelector('fieldset')!.disabled).toBe(false)
  expect(native).toHaveBeenCalledWith(responsiveTime,0,4000000,250000,expect.any(AbortSignal))
  document.querySelector<HTMLButtonElement>('[aria-label="Слой 1 add text key"]')!.click()
  expect(onchange.mock.lastCall![0][0].content.sourceTextAnimation.keyframes).toEqual([{tick:-500000,value:'Текст'}])
  native.mockRestore()
})
