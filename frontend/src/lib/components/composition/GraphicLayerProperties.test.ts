// @ts-expect-error Client mount required under Vitest SSR conditions.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import Editor from './GraphicLayerProperties.svelte'
import type { GraphicContent } from '$lib/composition/types'
import { graphicShapeChannels, graphicTextChannels, graphicNumericValue } from '$lib/composition/graphicLayerProperties'
for (const kind of ['shape', 'text'] as const) {
  it(`authors all original ${kind} numeric channels without altering sibling components or input data`, async () => {
    const channels = kind === 'shape' ? graphicShapeChannels : graphicTextChannels
    expect(channels).toHaveLength(kind === 'shape' ? 42 : 44)
    const content: GraphicContent = {kind,parameters:{position:[7,9],fillColor:[.1,.2,.3,.4]}}
    const before = JSON.stringify(content)
    const onchange = vi.fn()
    const target = document.createElement('div'); document.body.append(target)
    const component = mount(Editor,{target,props:{content,label:'layer',width:80,height:64,onchange}})
    await tick()
    for (const [channel,, min, max, fallback] of channels) {
      const input = target.querySelector<HTMLInputElement>(`input[aria-label="layer ${channel}"]`)!
      expect(input).not.toBeNull()
      const value = Math.min(max ?? Infinity, Math.max(min ?? -Infinity, fallback + .25))
      input.value=String(value);input.dispatchEvent(new Event('change',{bubbles:true}))
      const updated = onchange.mock.lastCall![0] as GraphicContent
      expect(graphicNumericValue(updated,channel,NaN)).toBe(value)
      if (channel==='positionX') expect(updated.parameters.position).toEqual([value,9])
      if (channel==='fillR') expect(updated.parameters.fillColor).toEqual([value,.2,.3,.4])
      expect(JSON.stringify(content)).toBe(before)
    }
    await unmount(component);target.remove()
  })
}
