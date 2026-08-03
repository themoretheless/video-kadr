import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => {
  const project = {
    id: 'project-1', name: 'Монтаж', videoId: 'asset-1', revision: 2,
    createdAt: 1, updatedAt: 2,
    video: { id: 'asset-1', assetId: 'asset-1', filename: 'clip.mp4', url: '', duration: 1, width: 10, height: 10 },
    edit: {}, document: { name: 'Монтаж', sequences: [] },
  }
  return {
    project,
    state: { projects: [project], video: project.video },
    timelineState: { revision: 1, document: { name: 'Монтаж' }, error: '' },
    flush: vi.fn(async () => undefined),
    load: vi.fn(async () => undefined),
    open: vi.fn(),
    exportArchive: vi.fn(),
    importArchive: vi.fn(),
    toast: vi.fn(),
  }
})

vi.mock('../store', () => ({
  clientOnlyMode: true,
  state: mocks.state,
  timelineState: mocks.timelineState,
  currentProjectId: () => 'project-1',
  flushProjectSave: mocks.flush,
  loadLibrary: mocks.load,
  openSavedProject: mocks.open,
}))
vi.mock('../project-archive-service', () => ({
  exportPortableProjectArchive: mocks.exportArchive,
  importPortableProjectArchive: mocks.importArchive,
}))
vi.mock('../toasts', () => ({ toast: mocks.toast }))

describe('ProjectArchivePanel', () => {
  let host: HTMLElement
  let app: ReturnType<typeof createApp> | null = null

  beforeEach(async () => {
    vi.clearAllMocks()
    mocks.state.projects = [mocks.project]
    mocks.timelineState.error = ''
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:archive') })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() })
    const Component = (await import('./ProjectArchivePanel.vue')).default
    host = document.createElement('div')
    document.body.append(host)
    app = createApp(Component)
    app.mount(host)
    await nextTick()
  })

  afterEach(() => {
    app?.unmount()
    host.remove()
    app = null
  })

  it('exports the selected project with originals enabled by default', async () => {
    mocks.exportArchive.mockResolvedValue({
      blob: new Blob(['archive']), filename: 'montage.vkadr', warnings: [], rootHash: 'a'.repeat(64),
    })
    const button = [...host.querySelectorAll('button')].find(item => item.textContent?.includes('Экспортировать'))!
    button.click()
    await vi.waitFor(() => expect(mocks.exportArchive).toHaveBeenCalledOnce(), { timeout: 3_000 })
    expect(mocks.flush).toHaveBeenCalledOnce()
    expect(mocks.exportArchive).toHaveBeenCalledWith(
      mocks.project,
      { includeOriginalMedia: true, includeProxies: false },
      expect.objectContaining({ signal: expect.any(AbortSignal), onProgress: expect.any(Function) }),
    )
  })

  it('refreshes the library and opens a verified imported project', async () => {
    const imported = { ...mocks.project, id: 'imported-1', name: 'Импорт' }
    mocks.importArchive.mockResolvedValue({
      project: imported, importedMedia: 1, reusedMedia: 0, missingMedia: [], warnings: [],
    })
    mocks.load.mockImplementationOnce(async () => { mocks.state.projects = [mocks.project, imported] })
    const file = new File(['portable'], 'project.vkadr', { type: 'application/vnd.video-kadr.project' })
    const input = host.querySelector('input[type="file"]') as HTMLInputElement
    Object.defineProperty(input, 'files', { configurable: true, value: [file] })
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(mocks.open).toHaveBeenCalledWith(imported), { timeout: 3_000 })
    expect(mocks.importArchive).toHaveBeenCalledWith(
      file,
      expect.objectContaining({ signal: expect.any(AbortSignal), onProgress: expect.any(Function) }),
    )
    expect(mocks.load).toHaveBeenCalledOnce()
  })
})
