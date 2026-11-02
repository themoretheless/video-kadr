// @ts-expect-error Vitest SSR conditions require the explicit Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import CompositionMediaLayer from './CompositionMediaLayer.svelte'
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })
it('drives amplified playback through a gain node and disconnects it when removed', async () => {
  const source = { connect: vi.fn(), disconnect: vi.fn() }
  const node = { connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }
  const context = { createMediaElementSource: vi.fn(() => source), createGain: vi.fn(() => node), destination: {}, currentTime: 0, state: 'suspended', resume: vi.fn().mockResolvedValue(undefined) }
  vi.stubGlobal('AudioContext', function () { return context })
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined)
  const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined)
  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(CompositionMediaLayer, { target, props: { kind: 'audio', url: '/test.wav', sourceTime: 0, playing: true, volume: 4 } })
  await tick()
  const element = target.querySelector('audio')!
  expect(element.volume).toBe(1)
  expect(context.createMediaElementSource).toHaveBeenCalledWith(element)
  expect(node.gain.setValueAtTime).toHaveBeenLastCalledWith(4, 0)
  expect(context.resume).toHaveBeenCalled()
  await unmount(component)
  expect(source.disconnect).toHaveBeenCalledOnce()
  expect(node.disconnect).toHaveBeenCalledOnce()
  expect(pause).toHaveBeenCalled()
  target.remove()
})
