import { chromium, expect, test } from '@playwright/test'
import type { CDPSession, Page } from '@playwright/test'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

function wavFixture(samples: number): Buffer {
  const dataBytes = samples * 2
  const buffer = Buffer.alloc(44 + dataBytes)
  buffer.write('RIFF', 0)
  buffer.writeUInt32LE(36 + dataBytes, 4)
  buffer.write('WAVEfmt ', 8)
  buffer.writeUInt32LE(16, 16)
  buffer.writeUInt16LE(1, 20)
  buffer.writeUInt16LE(1, 22)
  buffer.writeUInt32LE(8_000, 24)
  buffer.writeUInt32LE(16_000, 28)
  buffer.writeUInt16LE(2, 32)
  buffer.writeUInt16LE(16, 34)
  buffer.write('data', 36)
  buffer.writeUInt32LE(dataBytes, 40)
  return buffer
}

async function captureRealFileHandle(page: Page, cdp: CDPSession, fixturePath: string): Promise<void> {
  await page.evaluate(() => {
    const target = document.createElement('div')
    target.id = 'fsa-capture-target'
    Object.assign(target.style, { position: 'fixed', inset: '8px auto auto 8px', width: '80px', height: '80px', zIndex: '99999' })
    target.addEventListener('dragover', (event) => event.preventDefault())
    target.addEventListener('drop', async (event) => {
      event.preventDefault()
      const item = event.dataTransfer?.items[0]
      ;(window as Window & { __videoKadrHandle?: FileSystemFileHandle }).__videoKadrHandle =
        await item?.getAsFileSystemHandle() as FileSystemFileHandle
    })
    document.body.append(target)
  })
  const dragData = { items: [], files: [fixturePath], dragOperationsMask: 1 }
  await cdp.send('Input.dispatchDragEvent', { type: 'dragEnter', x: 20, y: 20, data: dragData })
  await cdp.send('Input.dispatchDragEvent', { type: 'drop', x: 20, y: 20, data: dragData })
  await expect.poll(() => page.evaluate(() => Boolean(
    (window as Window & { __videoKadrHandle?: FileSystemFileHandle }).__videoKadrHandle,
  ))).toBe(true)
}

async function forceFsaRelink(page: Page, assetId: string): Promise<void> {
  await page.evaluate(async (id) => {
    Object.defineProperty(FileSystemFileHandle.prototype, 'createWritable', {
      configurable: true,
      value: async () => { throw new DOMException('forced OPFS failure', 'UnknownError') },
    })
    const handle = (window as Window & { __videoKadrHandle: FileSystemFileHandle }).__videoKadrHandle
    const assetStore = await import('/src/browser-asset-store.ts')
    await assetStore.relinkBrowserAsset(id, await handle.getFile(), handle)
  }, assetId)
}

test('quota exhaustion preserves committed media and makes only the new upload session-only', async ({
  page, context, browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Quota override is a Chromium CDP gate')
  await page.goto('/?processing=browser')
  const input = page.locator('.dropzone input[type=file]')
  await input.setInputFiles({ name: 'durable.wav', mimeType: 'audio/wav', buffer: wavFixture(800) })
  await expect(page.locator('.lib-item')).toHaveCount(1)

  const cdp = await context.newCDPSession(page)
  await cdp.send('Storage.overrideQuotaForOrigin', {
    origin: new URL(page.url()).origin,
    quotaSize: 64 * 1024,
  })
  await input.setInputFiles({ name: 'over-quota.wav', mimeType: 'audio/wav', buffer: wavFixture(150_000) })
  await expect(page.locator('.lib-item')).toHaveCount(2)
  await expect(page.getByText(/файл доступен только до закрытия этой вкладки/)).toBeVisible()

  await page.reload({ waitUntil: 'networkidle' })
  await expect(page.locator('.lib-item')).toHaveCount(1)
  await expect(page.locator('.lib-item')).toContainText('durable.wav')
  await expect(page.locator('.lib-item')).not.toContainText('over-quota.wav')
})

test('selective OPFS eviction becomes offline and exact relink restores the same asset', async ({
  page, context, browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Selective file_systems eviction uses Chromium CDP')
  await page.goto('/?processing=browser')
  const original = { name: 'evicted.wav', mimeType: 'audio/wav', buffer: wavFixture(1_200) }
  await page.locator('.dropzone input[type=file]').setInputFiles(original)
  await expect(page.locator('.lib-item')).toHaveCount(1)
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr-media')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const getAll = database.transaction('manifests').objectStore('manifests').getAll()
    const manifests = await new Promise<Array<{ storage: string }>>((resolve, reject) => {
      getAll.onsuccess = () => resolve(getAll.result)
      getAll.onerror = () => reject(getAll.error)
    })
    database.close()
    return manifests[0]?.storage
  })).toBe('opfs')

  const cdp = await context.newCDPSession(page)
  await cdp.send('Storage.clearDataForOrigin', {
    origin: new URL(page.url()).origin,
    storageTypes: 'file_systems',
  })
  await page.reload({ waitUntil: 'networkidle' })
  const item = page.locator('.lib-item').filter({ hasText: 'evicted.wav' })
  await expect(item.getByRole('status')).toContainText('Файл недоступен')

  const relink = item.locator('input[type=file]')
  await relink.setInputFiles({ name: 'wrong.wav', mimeType: 'audio/wav', buffer: wavFixture(1_201) })
  await expect(item.getByRole('status')).toContainText('Файл недоступен')
  await relink.setInputFiles(original)
  await expect(item.getByRole('status')).toHaveCount(0)
  await page.reload({ waitUntil: 'networkidle' })
  await expect(page.locator('.lib-item').filter({ hasText: 'evicted.wav' }).getByRole('status')).toHaveCount(0)
})

test('project opens with missing primary and batch ignores wrong files while preserving topology', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'Selective OPFS object eviction is a Chromium recovery gate')
  await page.goto('/?processing=browser')
  const files = [
    { name: 'batch-primary.wav', mimeType: 'audio/wav', buffer: wavFixture(1_100) },
    { name: 'batch-second.wav', mimeType: 'audio/wav', buffer: wavFixture(1_200) },
    { name: 'batch-third.wav', mimeType: 'audio/wav', buffer: wavFixture(1_300) },
  ]
  await page.locator('.dropzone input[type=file]').setInputFiles(files)
  await expect(page.locator('.lib-item')).toHaveCount(3)
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const read = database.transaction('projects').objectStore('projects').getAll()
    const projects = await new Promise<Array<{ id: string; revision: number; document?: { media: Array<{ assetRef?: string; id: string }> } }>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error)
    })
    database.close()
    const project = projects[0]
    return project?.document?.media.length === 3 ? {
      id: project.id, revision: project.revision,
      refs: project.document.media.map((media) => media.assetRef ?? media.id),
    } : null
  }), { timeout: 10_000 }).not.toBeNull()
  const originalProject = await page.evaluate(async () => {
    const request = indexedDB.open('video-kadr')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const read = database.transaction('projects').objectStore('projects').getAll()
    const projects = await new Promise<Array<{ id: string; revision: number; document: { media: Array<{ assetRef?: string; id: string }> } }>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error)
    })
    database.close(); const project = projects[0]!
    return { id: project.id, revision: project.revision, refs: project.document.media.map((media) => media.assetRef ?? media.id) }
  })
  await page.evaluate(async () => {
    const request = indexedDB.open('video-kadr-media')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const read = database.transaction('manifests').objectStore('manifests').getAll()
    const manifests = await new Promise<Array<{ filename: string; objectKey: string }>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error)
    })
    database.close()
    const root = await navigator.storage.getDirectory()
    const directory = await root.getDirectoryHandle('video-kadr-media')
    for (const manifest of manifests.filter((item) => item.filename !== 'batch-second.wav')) {
      await directory.removeEntry(manifest.objectKey)
    }
  })
  await page.reload({ waitUntil: 'networkidle' })
  await page.getByRole('button', { name: /Открыть проект/ }).click()
  await expect(page.getByRole('region', { name: 'Временная шкала проекта' })).toBeVisible()
  await expect(page.locator('.timeline-clip')).toHaveCount(3)
  const summary = page.locator('.relink-summary')
  await expect(summary).toContainText('Недоступно исходников: 2')
  const wrong = { name: 'unrelated.wav', mimeType: 'audio/wav', buffer: wavFixture(777) }
  await summary.locator('input[type=file]').setInputFiles(wrong)
  await expect(summary).toContainText('Осталось найти: 2')
  const unchangedAfterWrong = await page.evaluate(async () => (await import('/src/browser-media.ts')).getProjects()
    .then((projects) => ({ id: projects[0]!.id, revision: projects[0]!.revision,
      refs: projects[0]!.document!.media.map((media) => media.assetRef ?? media.id) })))
  expect(unchangedAfterWrong).toEqual(originalProject)
  await summary.locator('input[type=file]').setInputFiles([wrong, files[2]!, files[0]!])
  await expect(summary).toHaveCount(0)
  await page.reload({ waitUntil: 'networkidle' })
  await expect(page.locator('.lib-item').getByRole('status')).toHaveCount(0)
  const after = await page.evaluate(async () => {
    const request = indexedDB.open('video-kadr')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const read = database.transaction('projects').objectStore('projects').getAll()
    const projects = await new Promise<Array<{ id: string; revision: number; document: { media: Array<{ assetRef?: string; id: string }> } }>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result); read.onerror = () => reject(read.error)
    })
    database.close(); const project = projects[0]!
    return { id: project.id, revision: project.revision, refs: project.document.media.map((media) => media.assetRef ?? media.id) }
  })
  expect(after).toEqual(originalProject)
})

test('ephemeral browser context survives reload but leaves no media in a new context', async ({ browser }) => {
  const baseURL = String(test.info().project.use.baseURL)
  const firstContext = await browser.newContext({ baseURL })
  const firstPage = await firstContext.newPage()
  await firstPage.goto('/?processing=browser')
  await firstPage.locator('.dropzone input[type=file]').setInputFiles({
    name: 'private-context.wav', mimeType: 'audio/wav', buffer: wavFixture(800),
  })
  await expect(firstPage.locator('.lib-item')).toHaveCount(1)
  await firstPage.reload({ waitUntil: 'networkidle' })
  await expect(firstPage.locator('.lib-item')).toHaveCount(1)
  await firstContext.close()

  const secondContext = await browser.newContext({ baseURL })
  const secondPage = await secondContext.newPage()
  await secondPage.goto('/?processing=browser')
  await expect(secondPage.locator('.lib-item')).toHaveCount(0)
  await secondContext.close()
})

test('a real external file handle survives reload and detects host file changes', async ({
  page, context, browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Real File System Access handles are Chromium-only')
  const fixtureDirectory = await mkdtemp(join(tmpdir(), 'video-kadr-fsa-'))
  const fixturePath = join(fixtureDirectory, 'external.wav')
  const original = wavFixture(1_400)
  try {
    await writeFile(fixturePath, original)
    await page.goto('/?processing=browser')
    await page.locator('.dropzone input[type=file]').setInputFiles(fixturePath)
    await expect(page.locator('.lib-item')).toHaveCount(1)
    const assetId = await page.evaluate(async () => (await import('/src/browser-media.ts')).getLibrary().then((items) => items[0]!.id))

    const cdp = await context.newCDPSession(page)
    await captureRealFileHandle(page, cdp, fixturePath)

    // Force only the durable-copy path to fail. The real external handle is
    // still structured-cloned into IndexedDB by the FSA fallback.
    await forceFsaRelink(page, assetId)
    await expect.poll(() => page.evaluate(async (id) => {
      const request = indexedDB.open('video-kadr-media')
      const database = await new Promise<IDBDatabase>((resolve, reject) => {
        request.onsuccess = () => resolve(request.result)
        request.onerror = () => reject(request.error)
      })
      const get = database.transaction('manifests').objectStore('manifests').get(id)
      const manifest = await new Promise<{ storage: string }>((resolve, reject) => {
        get.onsuccess = () => resolve(get.result)
        get.onerror = () => reject(get.error)
      })
      database.close()
      return manifest.storage
    }, assetId)).toBe('fsa')

    await page.reload({ waitUntil: 'networkidle' })
    await expect(page.locator('.lib-item')).toHaveCount(1)
    await expect(page.evaluate(async (id) => (await import('/src/browser-media.ts')).resolveSource(id).then(() => true), assetId)).resolves.toBe(true)

    const changed = Buffer.from(original)
    changed[changed.length - 1] = changed[changed.length - 1] === 0 ? 1 : 0
    await writeFile(fixturePath, changed)
    await page.reload({ waitUntil: 'networkidle' })
    const failure = await page.evaluate(async (id) => {
      try {
        await (await import('/src/browser-media.ts')).resolveSource(id)
        return null
      } catch (error) {
        return { name: (error as Error).name, reason: (error as { reason?: string }).reason }
      }
    }, assetId)
    expect(failure).toEqual({ name: 'BrowserAssetStorageError', reason: 'fingerprint' })
  } finally {
    await rm(fixtureDirectory, { recursive: true, force: true })
  }
})

test('persistent browser restart fails closed when external-handle permission is lost', async ({ browserName }) => {
  test.skip(browserName !== 'chromium', 'Persistent File System Access handles are Chromium-only')
  const baseURL = String(test.info().project.use.baseURL)
  const fixtureDirectory = await mkdtemp(join(tmpdir(), 'video-kadr-fsa-restart-'))
  const profileDirectory = await mkdtemp(join(tmpdir(), 'video-kadr-profile-'))
  const fixturePath = join(fixtureDirectory, 'restart.wav')
  let firstContext: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  let secondContext: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  try {
    await writeFile(fixturePath, wavFixture(1_000))
    firstContext = await chromium.launchPersistentContext(profileDirectory, { headless: true, baseURL })
    const firstPage = firstContext.pages()[0] ?? await firstContext.newPage()
    await firstPage.goto('/?processing=browser')
    await firstPage.locator('.dropzone input[type=file]').setInputFiles(fixturePath)
    await expect(firstPage.locator('.lib-item')).toHaveCount(1)
    const assetId = await firstPage.evaluate(async () => (await import('/src/browser-media.ts')).getLibrary().then((items) => items[0]!.id))
    await captureRealFileHandle(firstPage, await firstContext.newCDPSession(firstPage), fixturePath)
    await forceFsaRelink(firstPage, assetId)
    await firstContext.close()
    firstContext = undefined

    secondContext = await chromium.launchPersistentContext(profileDirectory, { headless: true, baseURL })
    const secondPage = secondContext.pages()[0] ?? await secondContext.newPage()
    await secondPage.goto('/?processing=browser')
    const item = secondPage.locator('.lib-item').filter({ hasText: 'restart.wav' })
    await expect(item.getByRole('status')).toContainText('Нужно снова разрешить доступ')
    const failure = await secondPage.evaluate(async (id) => {
      try {
        await (await import('/src/browser-media.ts')).resolveSource(id)
        return null
      } catch (error) {
        return { name: (error as Error).name, reason: (error as { reason?: string }).reason }
      }
    }, assetId)
    expect(failure).toEqual({ name: 'BrowserAssetStorageError', reason: 'missing' })
  } finally {
    await firstContext?.close()
    await secondContext?.close()
    await rm(fixtureDirectory, { recursive: true, force: true })
    await rm(profileDirectory, { recursive: true, force: true })
  }
})

test('cross-tab delete and alias ingest never garbage-collect the surviving digest', async ({ page, context }) => {
  await page.goto('/?processing=browser')
  const secondPage = await context.newPage()
  await secondPage.goto('/?processing=browser')
  const storeAsset = async (target: Page, id: string) => target.evaluate(async (assetId) => {
    const bytes = 'shared-cross-tab-digest'
    const file = new File([bytes], `${assetId}.mp4`, { type: 'video/mp4' })
    return (await import('/src/browser-asset-store.ts')).putBrowserAsset({
      id: assetId, file, filename: file.name, fileType: file.type,
      info: { id: assetId, filename: file.name, duration: 1, width: 10, height: 10, mediaKind: 'video' },
      createdAt: 1,
    })
  }, id)
  const first = await storeAsset(page, 'cross-tab-delete')
  const [, survivor] = await Promise.all([
    page.evaluate(async () => (await import('/src/browser-asset-store.ts')).deleteBrowserAsset('cross-tab-delete')),
    storeAsset(secondPage, 'cross-tab-survivor'),
  ])
  expect(survivor.objectKey).toBe(first.objectKey)
  const restored = await secondPage.evaluate(async () => {
    const asset = await (await import('/src/browser-asset-store.ts')).getBrowserAsset('cross-tab-survivor')
    return { id: asset.id, text: await asset.file.text() }
  })
  expect(restored).toEqual({ id: 'cross-tab-survivor', text: 'shared-cross-tab-digest' })
  await secondPage.close()
})
