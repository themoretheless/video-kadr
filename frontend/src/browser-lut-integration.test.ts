import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it, vi } from 'vitest'

import { closeBrowserLutLibrary } from './browser-lut-library'

async function clearLuts(): Promise<void> {
  await closeBrowserLutLibrary()
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr-luts')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

const identity = `LUT_3D_SIZE 2
0 0 0
1 0 0
0 1 0
1 1 0
0 0 1
1 0 1
0 1 1
1 1 1
`

describe('browser LUT media integration', () => {
  beforeEach(async () => {
    await clearLuts()
    vi.resetModules()
  })

  it('strictly imports, favorites and restores canonical LUT bytes after reload', async () => {
    let media = await import('./browser-media')
    await expect(media.uploadLut(new File(['LUT_3D_SIZE 2\n0 0 0\n'], 'broken.cube')))
      .rejects.toThrow(/wrong_entry_count/)

    const uploaded = await media.uploadLut(new File([identity], 'Cinema.cube'))
    expect(uploaded).toMatchObject({ name: 'Cinema.cube', cubeSize: 2 })
    await expect(media.setLutFavorite(uploaded.id, true)).resolves.toMatchObject({ favorite: true })

    await (await import('./browser-lut-library')).closeBrowserLutLibrary()
    vi.resetModules()
    media = await import('./browser-media')
    await expect(media.listLuts({ favorite: true })).resolves.toEqual([
      expect.objectContaining({ id: uploaded.id, favorite: true }),
    ])
    await expect((await media.getLutContent(uploaded.id)).text())
      .resolves.toContain('DOMAIN_MIN 0 0 0')
    await expect(media.getLut(uploaded.id)).resolves.toMatchObject({ sha256: uploaded.sha256 })

    const baked = await media.bakeLut({
      edit: { brightness: 0.1, contrast: 1.2, saturation: 0.8, filter: 'sepia' },
      size: 33,
    })
    expect(baked.filename).toBe('video-kadr-look-33.cube')
    await expect(baked.blob.text()).resolves.toContain('LUT_3D_SIZE 33')

    window.history.replaceState({}, '', '/?processing=browser')
    vi.resetModules()
    const publicApi = await import('./api')
    expect(publicApi.clientOnlyMode).toBe(true)
    await expect(publicApi.bakeLut({ edit: {}, size: 33 })).resolves.toMatchObject({
      filename: 'video-kadr-look-33.cube',
    })
    window.history.replaceState({}, '', '/')
  })
})
