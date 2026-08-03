import { describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  serverExport: vi.fn(),
  serverImport: vi.fn(),
  browserExport: vi.fn(),
  browserImport: vi.fn(),
}))

vi.mock('./api', () => ({
  clientOnlyMode: false,
  exportProjectArchive: mocks.serverExport,
  importProjectArchive: mocks.serverImport,
}))
vi.mock('./browser-project-archive', () => ({
  exportBrowserProjectArchive: mocks.browserExport,
  importBrowserProjectArchive: mocks.browserImport,
}))

import { exportPortableProjectArchive, importPortableProjectArchive } from './project-archive-service'

describe('project archive runtime service in server mode', () => {
  it('routes export to the server transport', async () => {
    const blob = new Blob(['archive'])
    mocks.serverExport.mockResolvedValue({ blob, filename: 'server.vkadr' })
    const signal = new AbortController().signal
    const onProgress = vi.fn()
    const project = { id: 'project-1' }

    await expect(exportPortableProjectArchive(project as never, {
      includeOriginalMedia: true,
      includeProxies: false,
    }, { signal, onProgress })).resolves.toEqual({ blob, filename: 'server.vkadr', warnings: [] })
    expect(mocks.serverExport).toHaveBeenCalledWith('project-1', {
      includeOriginalMedia: true,
      includeProxies: false,
    }, signal)
    expect(mocks.browserExport).not.toHaveBeenCalled()
    expect(onProgress).toHaveBeenLastCalledWith(expect.objectContaining({ phase: 'done' }))
  })

  it('routes raw import to the server and normalizes its response', async () => {
    mocks.serverImport.mockResolvedValue({ projectId: 'copy-1', revision: 1, missingMedia: ['media-1'] })
    const archive = new Blob(['archive'])

    await expect(importPortableProjectArchive(archive)).resolves.toEqual({
      projectId: 'copy-1',
      importedMedia: 0,
      reusedMedia: 0,
      missingMedia: ['media-1'],
      warnings: [],
    })
    expect(mocks.serverImport).toHaveBeenCalledWith(archive, undefined)
    expect(mocks.browserImport).not.toHaveBeenCalled()
  })
})
