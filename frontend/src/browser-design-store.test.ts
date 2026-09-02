import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it } from 'vitest'

import type { BrandKit } from './domain/brand-kit'
import type { ProjectTemplate } from './domain/project-template'
import { migrateProjectDocument } from './project-schema'
import {
  deleteBrandKit,
  deleteDesignAsset,
  DesignAssetError,
  DesignConflictError,
  DesignDependencyError,
  getBrandKit,
  getDesignAsset,
  getProjectTemplate,
  listBrandKits,
  listProjectTemplates,
  putDesignAsset,
  saveBrandKit,
  saveProjectTemplate,
} from './browser-design-store'

function clearDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr-design')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

function kit(id = 'kit-1', revision = 1): BrandKit {
  return { schemaVersion: 1, id, revision, name: 'Studio', colors: [], fonts: [], logos: [], defaults: {} }
}

function template(id = 'template-1', revision = 1, brandKitId?: string): ProjectTemplate {
  const document = migrateProjectDocument({
    schemaVersion: 1,
    videoId: 'placeholder-media',
    name: 'Template',
    video: { id: 'placeholder-media', filename: 'placeholder.mp4', duration: 1, width: 1920, height: 1080, fingerprint: 'c'.repeat(64), sizeBytes: 100 },
    edit: {},
  })
  return {
    schemaVersion: 1, id, revision, name: 'Template', sourceProjectSchemaVersion: 4,
    document, placeholders: [], requiredAssets: [{ kind: 'media', assetRef: 'placeholder-media', fingerprint: 'c'.repeat(64), mimeType: 'application/octet-stream', byteLength: 100 }],
    ...(brandKitId ? { brandKitPin: { id: brandKitId, revision: 1 }, brandKitSnapshot: kit(brandKitId) } : {}),
  }
}

async function rawPut(storeName: string, value: unknown): Promise<void> {
  const database = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open('video-kadr-design', 2)
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(storeName, 'readwrite')
      transaction.objectStore(storeName).put(value)
      transaction.oncomplete = () => resolve()
      transaction.onerror = () => reject(transaction.error)
    })
  } finally { database.close() }
}

describe('browser design persistence', () => {
  beforeEach(clearDatabase)

  it('provides revision-CAS CRUD for templates and brand kits', async () => {
    await saveProjectTemplate(template(), 0)
    await saveBrandKit(kit(), 0)
    expect((await getProjectTemplate('template-1'))?.revision).toBe(1)
    expect((await getBrandKit('kit-1'))?.value.name).toBe('Studio')

    await expect(saveBrandKit({ ...kit(), name: 'Stale' }, 0)).rejects.toBeInstanceOf(DesignConflictError)
    const updated = await saveBrandKit({ ...kit(), revision: 2, name: 'Updated' }, 1)
    expect(updated).toMatchObject({ revision: 2, value: { name: 'Updated', revision: 2 } })
    expect(await getBrandKit('kit-1', 1)).toMatchObject({ revision: 1, value: { name: 'Studio' } })
    expect(await getBrandKit('kit-1', 2)).toMatchObject({ revision: 2, value: { name: 'Updated' } })
    expect(await getBrandKit('kit-1', 3)).toBeNull()
    expect(await listProjectTemplates()).toHaveLength(1)
    expect(await listBrandKits()).toHaveLength(1)
  })

  it('validates before commit and quarantines malformed rows on read', async () => {
    await expect(saveBrandKit({ ...kit(), name: '' }, 0)).rejects.toThrow('invalid brand kit name')
    expect(await listBrandKits()).toEqual([])
    await saveBrandKit(kit('healthy'), 0)
    await rawPut('brand-kits', { id: 'corrupt', revision: 1, createdAt: 1, updatedAt: 1, value: { schemaVersion: 99, id: 'corrupt' } })
    expect((await listBrandKits()).map(row => row.id)).toEqual(['healthy'])
    expect(await getBrandKit('corrupt')).toBeNull()
  })

  it('blocks deletion of a brand kit pinned by a template', async () => {
    await saveBrandKit(kit(), 0)
    await saveProjectTemplate(template('uses-kit', 1, 'kit-1'), 0)
    await expect(deleteBrandKit('kit-1')).rejects.toBeInstanceOf(DesignDependencyError)
    expect(await getBrandKit('kit-1')).not.toBeNull()
  })

  it('stores verified content-addressed font bytes and deduplicates aliases', async () => {
    const bytes = new Uint8Array([119, 79, 70, 50, 0, 0, 0, 0])
    const font = new Blob([bytes], { type: 'font/woff2' })
    const first = await putDesignAsset('font-1', 'Heading', 'font', font)
    const second = await putDesignAsset('font-2', 'Body', 'font', font)
    expect(second.fingerprint).toBe(first.fingerprint)
    await expect((await getDesignAsset('font-2'))?.blob.arrayBuffer()).resolves.toEqual(bytes.buffer)
    expect(await deleteDesignAsset('font-1')).toBe(true)
    expect(await getDesignAsset('font-2')).not.toBeNull()
  })

  it('rejects MIME/signature confusion without a partial manifest', async () => {
    const fake = new Blob([new Uint8Array([0, 1, 2, 3])], { type: 'image/png' })
    await expect(putDesignAsset('logo-1', 'Logo', 'logo', fake)).rejects.toMatchObject({ reason: 'type' } satisfies Partial<DesignAssetError>)
    expect(await getDesignAsset('logo-1')).toBeNull()
  })

  it('prevents replacement of an asset id with different bytes', async () => {
    const first = new Blob([new Uint8Array([119, 79, 70, 50, 1])], { type: 'font/woff2' })
    const second = new Blob([new Uint8Array([119, 79, 70, 50, 2])], { type: 'font/woff2' })
    await putDesignAsset('font-1', 'Font', 'font', first)
    await expect(putDesignAsset('font-1', 'Font', 'font', second)).rejects.toBeInstanceOf(DesignConflictError)
    expect((await getDesignAsset('font-1'))?.manifest.fingerprint).toBe((await putDesignAsset('font-2', 'Font', 'font', first)).fingerprint)
  })

  it('does not delete bytes referenced by a validated brand kit', async () => {
    const png = new Blob([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10])], { type: 'image/png' })
    const manifest = await putDesignAsset('logo-1', 'Logo', 'logo', png)
    await saveBrandKit({
      ...kit(),
      logos: [{ id: 'logo-token', name: 'Logo', variant: 'primary', asset: {
        assetRef: manifest.id, fingerprint: manifest.fingerprint, mimeType: manifest.mimeType, byteLength: manifest.byteLength,
      } }],
    }, 0)
    await expect(deleteDesignAsset('logo-1')).rejects.toBeInstanceOf(DesignDependencyError)
    expect(await getDesignAsset('logo-1')).not.toBeNull()
  })

  it('integrates uploaded font manifests with strict BrandKit persistence', async () => {
    const { createBrandKit, designState } = await import('./design-store')
    await createBrandKit({
      name: 'Integrated', colors: [],
      fontFiles: [new File([new Uint8Array([119, 79, 70, 50])], 'Brand.woff2', { type: 'font/woff2' })],
      logoFiles: [],
    })
    expect(designState.error).toBe('')
    const [stored] = await listBrandKits()
    expect(stored?.value.fonts[0]?.asset).toEqual({
      assetRef: expect.stringMatching(/^font-/), fingerprint: expect.stringMatching(/^[a-f0-9]{64}$/), mimeType: 'font/woff2', byteLength: 4,
    })
  })
})
