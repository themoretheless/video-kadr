import { expect, test } from '@playwright/test'

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
