// @ts-expect-error Tests need Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import { getLibraryAnalysisFrame } from '../api.js'
import ShotCover from './ShotCover.svelte'

vi.mock('../api.js', () => ({ getLibraryAnalysisFrame: vi.fn() }))

it('loads a cover on visibility and cancels and releases it when it leaves view', async () => {
  let visibility!: (inside: boolean) => void
  const disconnect = vi.fn()
  vi.stubGlobal('IntersectionObserver', class {
    constructor(callback: IntersectionObserverCallback) {
      visibility = inside => callback([{ isIntersecting: inside } as IntersectionObserverEntry], this as unknown as IntersectionObserver)
    }
    observe() {}
    disconnect = disconnect
  })
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:cover')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const image = new Blob(['mock PNG'], { type: 'image/png' })
  let late!: (blob: Blob) => void
  const fetchFrame = vi.mocked(getLibraryAnalysisFrame)
  fetchFrame.mockResolvedValueOnce(image).mockImplementationOnce(() => new Promise(resolve => { late = resolve }))
  const onseek = vi.fn()
  const target = document.createElement('div')
  const component = mount(ShotCover, { target, props: {
    sourceId: 'source', sourceTick: 123456, token: 'owner', spaceId: 'space', label: 'Кадр сцены 2', onseek,
  } })
  try {
    await tick()
    expect(fetchFrame).not.toHaveBeenCalled()
    visibility(true)
    await vi.waitFor(() => expect(target.querySelector('button')?.getAttribute('aria-label')).toBe('Перейти к кадру · Кадр сцены 2'))
    target.querySelector('button')!.click()
    expect(onseek).toHaveBeenCalledOnce()
    expect(fetchFrame).toHaveBeenCalledWith('source', 123456, 'owner', 'space', expect.any(AbortSignal))
    visibility(false)
    await tick()
    expect(target.querySelector('img')).toBeNull()
    expect(revoke).toHaveBeenCalledWith('blob:cover')
    visibility(true)
    await tick()
    const nextSignal = fetchFrame.mock.calls[1]?.[4]
    visibility(false)
    await tick()
    expect(nextSignal?.aborted).toBe(true)
    late(image)
    await tick()
    expect(create).toHaveBeenCalledOnce()
  } finally {
    await unmount(component)
    expect(disconnect).toHaveBeenCalledOnce()
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
    fetchFrame.mockReset()
  }
})
