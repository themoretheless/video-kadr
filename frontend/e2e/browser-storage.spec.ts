import { expect, test } from '@playwright/test'

function wavFixture(index: number): Buffer {
  const sampleRate = 8_000
  const samples = 800 + index
  const dataBytes = samples * 2
  const buffer = Buffer.alloc(44 + dataBytes)
  buffer.write('RIFF', 0)
  buffer.writeUInt32LE(36 + dataBytes, 4)
  buffer.write('WAVEfmt ', 8)
  buffer.writeUInt32LE(16, 16)
  buffer.writeUInt16LE(1, 20)
  buffer.writeUInt16LE(1, 22)
  buffer.writeUInt32LE(sampleRate, 24)
  buffer.writeUInt32LE(sampleRate * 2, 28)
  buffer.writeUInt16LE(2, 32)
  buffer.writeUInt16LE(16, 34)
  buffer.write('data', 36)
  buffer.writeUInt32LE(dataBytes, 40)
  for (let offset = 44; offset < buffer.length; offset += 2) buffer.writeInt16LE(index, offset)
  return buffer
}

test('local assets and their project survive a hard browser reload', async ({ page, browserName }) => {
  test.slow()
  await page.goto('/?processing=browser')
  const assetCount = browserName === 'webkit' ? 2 : 20
  const files = Array.from({ length: assetCount }, (_, index) => ({
    name: `fixture-${index.toString().padStart(2, '0')}.wav`,
    mimeType: 'audio/wav',
    buffer: wavFixture(index),
  }))
  await page.locator('.dropzone input[type=file]').setInputFiles(files)
  await expect.poll(async () => ({
    count: await page.locator('.lib-item').count(),
    importing: await page.locator('.dropzone').isDisabled(),
    error: await page.locator('.import p.error:not([role=status])').allTextContents(),
  }), { timeout: 90_000 }).toEqual({ count: assetCount, importing: false, error: [] })
  await expect(page.locator('.lib-item')).toHaveCount(assetCount)

  const sessionOnly = page.getByText(/файл доступен только до закрытия этой вкладки/)
  if (await sessionOnly.isVisible()) {
    expect(browserName).toBe('webkit')
    await page.reload({ waitUntil: 'networkidle' })
    await expect(page.locator('.lib-item')).toHaveCount(0)
    return
  }

  await expect.poll(() => page.evaluate(async () =>
    (await indexedDB.databases()).some((database) => database.name === 'video-kadr'),
  ), { timeout: 10_000 }).toBe(true)

  const before = await page.evaluate(async () => {
    const request = indexedDB.open('video-kadr')
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('projects', 'readonly')
    const getAll = transaction.objectStore('projects').getAll()
    const projects = await new Promise<Array<{ document?: { media: unknown[] } }>>((resolve, reject) => {
      getAll.onsuccess = () => resolve(getAll.result)
      getAll.onerror = () => reject(getAll.error)
    })
    database.close()
    return projects[0]?.document?.media.length ?? 0
  })
  expect(before).toBe(assetCount)

  await page.reload({ waitUntil: 'networkidle' })
  await expect(page.locator('.lib-item')).toHaveCount(assetCount, { timeout: 30_000 })
  await expect(page.locator('.lib-item video, .lib-item audio')).toHaveCount(0)
  const primary = page.locator('.lib-item').filter({ hasText: 'fixture-00.wav' })
  await primary.getByRole('button', { name: 'Открыть как проект' }).click()
  await expect(page.getByRole('heading', { name: /fixture-00.wav/ })).toBeVisible({ timeout: 30_000 })

  // The same scenario is intentionally exercised by every configured engine.
  expect(['chromium', 'firefox', 'webkit']).toContain(browserName)
})
