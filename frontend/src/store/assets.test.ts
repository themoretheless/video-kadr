import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as api from '../api'
import type { AssetEntry } from '../types'
import {
  assetsState,
  assetUrl,
  deleteAsset,
  loadAssets,
  MAX_ASSET_UPLOAD_BYTES,
  resetAssets,
  sanitizeAssetEntry,
  uploadAsset,
} from './assets'

vi.mock('../api', () => {
  class ApiError extends Error {
    constructor(
      message: string,
      readonly status: number,
      readonly code?: string,
    ) {
      super(message)
    }
  }
  class BackendUnavailableError extends Error {
    constructor() {
      super('Сервер недоступен')
    }
  }
  class AssetsRequireServerError extends Error {}

  return {
    clientOnlyMode: false,
    ApiError,
    BackendUnavailableError,
    AssetsRequireServerError,
    getAssets: vi.fn(() => Promise.resolve([])),
    uploadAsset: vi.fn(),
    deleteAsset: vi.fn(() => Promise.resolve()),
  }
})

vi.mock('../toasts', () => ({ toast: vi.fn() }))

const ASSET: AssetEntry = {
  id: 'ast_abcdefghijklmnop',
  kind: 'image',
  filename: 'logo.png',
  mime: 'image/png',
  sizeBytes: 2048,
  sha256: 'a'.repeat(64),
  width: 512,
  height: 512,
  duration: null,
}

/** A File stub: happy-dom's File is enough, only name/size/type are read. */
function fileOfSize(bytes: number): File {
  const file = new File(['x'], 'logo.png', { type: 'image/png' })
  Object.defineProperty(file, 'size', { value: bytes })
  return file
}

beforeEach(() => {
  resetAssets()
  vi.mocked(api.getAssets).mockReset().mockResolvedValue([])
  vi.mocked(api.uploadAsset).mockReset()
  vi.mocked(api.deleteAsset).mockReset().mockResolvedValue(undefined)
})

describe('sanitizeAssetEntry', () => {
  it('accepts a well-formed entry', () => {
    expect(sanitizeAssetEntry({ ...ASSET })).toEqual(ASSET)
  })

  it('rejects ids outside the ast_ allow-list', () => {
    expect(sanitizeAssetEntry({ ...ASSET, id: '../../etc/passwd' })).toBeNull()
    expect(sanitizeAssetEntry({ ...ASSET, id: 'ast_short' })).toBeNull()
  })

  it('rejects unknown kinds and missing sizes', () => {
    expect(sanitizeAssetEntry({ ...ASSET, kind: 'executable' })).toBeNull()
    expect(sanitizeAssetEntry({ ...ASSET, sizeBytes: 'huge' })).toBeNull()
  })

  it('nulls out non-finite dimensions', () => {
    const asset = sanitizeAssetEntry({ ...ASSET, width: Number.NaN, duration: Number.POSITIVE_INFINITY })
    expect(asset?.width).toBeNull()
    expect(asset?.duration).toBeNull()
  })
})

describe('loadAssets', () => {
  it('keeps only entries that survive validation', async () => {
    vi.mocked(api.getAssets).mockResolvedValue([
      ASSET,
      { ...ASSET, id: 'nope' },
    ] as AssetEntry[])

    await loadAssets()

    expect(assetsState.list).toEqual([ASSET])
    expect(assetsState.loadError).toBe('')
  })

  it('reports a transport failure without throwing', async () => {
    vi.mocked(api.getAssets).mockRejectedValue(new api.BackendUnavailableError())

    await loadAssets()

    expect(assetsState.list).toEqual([])
    expect(assetsState.loadError).toBe('Сервер недоступен')
  })
})

describe('uploadAsset', () => {
  it('rejects an oversized file before touching the network', async () => {
    const result = await uploadAsset(fileOfSize(MAX_ASSET_UPLOAD_BYTES.image + 1), 'image')

    expect(result).toBeNull()
    expect(api.uploadAsset).not.toHaveBeenCalled()
    expect(assetsState.uploadError).toContain('лимита')
  })

  it('rejects an empty file', async () => {
    const result = await uploadAsset(fileOfSize(0), 'image')

    expect(result).toBeNull()
    expect(api.uploadAsset).not.toHaveBeenCalled()
    expect(assetsState.uploadError).toBe('Файл пуст')
  })

  it('prepends the stored asset and forwards upload progress', async () => {
    const progress: number[] = []
    vi.mocked(api.uploadAsset).mockImplementation(async (_file, _kind, options) => {
      options?.onProgress?.(40)
      progress.push(assetsState.uploadProgress ?? -1)
      options?.onProgress?.(100)
      progress.push(assetsState.uploadProgress ?? -1)
      return ASSET
    })
    assetsState.list = [{ ...ASSET, id: 'ast_zzzzzzzzzzzzzzzz' }]

    const result = await uploadAsset(fileOfSize(1024), 'image')

    expect(progress).toEqual([40, 100])
    expect(result).toEqual(ASSET)
    expect(assetsState.list[0]).toEqual(ASSET)
    expect(assetsState.list).toHaveLength(2)
    expect(assetsState.uploading).toBe(false)
    expect(assetsState.uploadProgress).toBeNull()
  })

  it('refuses a malformed server response', async () => {
    vi.mocked(api.uploadAsset).mockResolvedValue({ id: 'bogus' } as unknown as AssetEntry)

    const result = await uploadAsset(fileOfSize(1024), 'image')

    expect(result).toBeNull()
    expect(assetsState.list).toEqual([])
    expect(assetsState.uploadError).toContain('некорректные данные')
  })

  it('surfaces a cancellation without an error banner', async () => {
    vi.mocked(api.uploadAsset).mockRejectedValue(new Error('cancelled'))

    const result = await uploadAsset(fileOfSize(1024), 'image')

    expect(result).toBeNull()
    expect(assetsState.uploadError).toBe('')
  })
})

describe('deleteAsset', () => {
  it('drops the entry from the list on success', async () => {
    assetsState.list = [ASSET]

    await expect(deleteAsset(ASSET.id)).resolves.toBe(true)
    expect(assetsState.list).toEqual([])
  })

  it('keeps the entry when the server refuses', async () => {
    assetsState.list = [ASSET]
    vi.mocked(api.deleteAsset).mockRejectedValue(new api.ApiError('Занят', 409))

    await expect(deleteAsset(ASSET.id)).resolves.toBe(false)
    expect(assetsState.list).toEqual([ASSET])
  })
})

describe('assetUrl', () => {
  it('points at the read-only assets mount', () => {
    expect(assetUrl(ASSET)).toBe('/files/assets/ast_abcdefghijklmnop')
  })
})
