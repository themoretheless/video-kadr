import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it } from 'vitest'

import {
  closeBrowserLutLibrary,
  getBrowserLut,
  listBrowserLuts,
  putBrowserLut,
  setBrowserLutFavorite,
} from './browser-lut-library'
import { encodeCanonicalCube, type RgbTuple } from './domain/cube-lut'

function deleteDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr-luts')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

function cube(invert = false): Uint8Array {
  const values: RgbTuple[] = []
  for (let blue = 0; blue < 2; blue++) {
    for (let green = 0; green < 2; green++) {
      for (let red = 0; red < 2; red++) {
        values.push(invert ? [1 - red, 1 - green, 1 - blue] : [red, green, blue])
      }
    }
  }
  return encodeCanonicalCube(2, values).bytes
}

describe('durable browser LUT library', () => {
  beforeEach(async () => {
    await closeBrowserLutLibrary()
    await deleteDatabase()
  })

  it('persists canonical bytes and metadata across database reopen', async () => {
    const stored = await putBrowserLut(cube(), '  Cinema\u0000  ', 'upload', 10)
    expect(stored).toMatchObject({ name: 'Cinema', cubeSize: 2, source: 'upload' })
    expect(stored.sha256).toMatch(/^[0-9a-f]{64}$/)
    await closeBrowserLutLibrary()
    const restored = await getBrowserLut(stored.id)
    expect(restored.asset).toEqual(stored)
    expect(new Uint8Array(await restored.blob.arrayBuffer())).toEqual(cube())
  })

  it('deduplicates canonical content independent of upload formatting and name', async () => {
    const first = await putBrowserLut(cube(), 'First')
    const decorated = `TITLE "Duplicate"\n# comment\n${new TextDecoder().decode(cube())}`
    const duplicate = await putBrowserLut(decorated, 'Second')
    expect(duplicate).toEqual(first)
    expect(await listBrowserLuts()).toHaveLength(1)
  })

  it('searches normalized names and keeps favorites separate from immutable assets', async () => {
    const older = await putBrowserLut(cube(), 'КИНО Лето', 'upload', 10)
    const newer = await putBrowserLut(cube(true), 'Night', 'baked', 20)
    await setBrowserLutFavorite(older.sha256, true)
    expect(await listBrowserLuts()).toMatchObject([
      { sha256: older.sha256, favorite: true },
      { sha256: newer.sha256, favorite: false },
    ])
    expect(await listBrowserLuts({ query: 'кино', favorite: true })).toMatchObject([
      { id: older.id, name: 'КИНО Лето' },
    ])
    expect((await getBrowserLut(older.id)).asset).not.toHaveProperty('favorite')
  })

  it('detects same-size byte corruption before returning a LUT', async () => {
    const stored = await putBrowserLut(cube(), 'Safe')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-luts', 1)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const bytes = cube().slice()
    bytes[bytes.length - 2] = bytes[bytes.length - 2] === 48 ? 49 : 48
    const transaction = database.transaction('blobs', 'readwrite')
    transaction.objectStore('blobs').put(bytes.buffer, stored.sha256)
    await new Promise<void>((resolve, reject) => {
      transaction.oncomplete = () => resolve()
      transaction.onerror = () => reject(transaction.error)
    })
    database.close()
    await expect(getBrowserLut(stored.id)).rejects.toMatchObject({ reason: 'integrity' })
  })
})
