// @ts-expect-error Tests need Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import SceneModelSetup from './SceneModelSetup.svelte'

it('installs through the authenticated action and polls for readiness', async () => {
  const phases = ['missing', 'installing', 'ready']
  const fetchMock = vi.fn(async (path: string, options?: RequestInit) => {
    expect(path).toBe('/api/scene-model')
    expect(new Headers(options?.headers).get('authorization')).toBe('Bearer operator')
    return Response.json({ model: 'qwen2.5vl:3b', phase: phases.shift(), canInstall: true })
  })
  vi.stubGlobal('fetch', fetchMock)
  const target = document.createElement('div')
  document.body.append(target)
  const panel = mount(SceneModelSetup, { target, props: { token: 'operator' } })
  const button = () => [...target.querySelectorAll('button')].find(item => item.textContent?.includes('Установить модель'))
  try {
    await vi.waitFor(() => expect(button(), target.textContent).toBeDefined())
    vi.useFakeTimers()
    button()!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Модель загружается'))
    expect(button()).toBeUndefined()
    await vi.advanceTimersByTimeAsync(3000)
    await tick()
    await vi.waitFor(() => expect(target.textContent).toContain('Модель готова к работе'))
    expect(fetchMock.mock.calls.map(([, options]) => options?.method ?? 'GET')).toEqual(['GET', 'POST', 'GET'])
  } finally {
    await unmount(panel)
    target.remove()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})
