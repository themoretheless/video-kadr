import { createApp, nextTick } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  relink: vi.fn(),
  state: {
    library: [{
      id: 'permission-source', kind: 'source', filename: 'source.mp4', url: '',
      duration: 1, availability: 'permission-required', createdAt: 1,
    }],
  },
  timelineState: { document: null },
}))

vi.mock('../store', () => ({
  state: mocks.state,
  timelineState: mocks.timelineState,
  openFromLibrary: vi.fn(),
  deleteFromLibrary: vi.fn(),
  addMediaToTimeline: vi.fn(),
  relinkLibraryMedia: mocks.relink,
  restoreExternalLibraryMedia: vi.fn(),
}))

describe('MediaLibrary File System Access relink', () => {
  let host: HTMLElement | null = null

  afterEach(() => {
    host?.remove()
    host = null
    Reflect.deleteProperty(window, 'showOpenFilePicker')
    vi.resetModules()
    mocks.relink.mockReset()
  })

  it('passes the selected file and its persistent handle to relink', async () => {
    const file = new File(['durable'], 'source.mp4', { type: 'video/mp4' })
    const handle = { getFile: vi.fn(async () => file) } as unknown as FileSystemFileHandle
    Object.defineProperty(window, 'showOpenFilePicker', {
      configurable: true,
      value: vi.fn(async () => [handle]),
    })
    const MediaLibrary = (await import('./MediaLibrary.vue')).default
    host = document.createElement('div')
    document.body.append(host)
    createApp(MediaLibrary).mount(host)

    const button = [...host.querySelectorAll('button')]
      .find((candidate) => candidate.textContent?.includes('Связать внешний файл'))
    expect(button).toBeDefined()
    button!.click()
    await nextTick()
    await vi.waitFor(() => expect(mocks.relink).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'permission-source' }), file, handle,
    ))
  })
})
