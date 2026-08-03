import { chromium, expect, test } from '@playwright/test'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

function wavFixture(): Buffer {
  const samples = 1_000
  const dataBytes = samples * 2
  const buffer = Buffer.alloc(44 + dataBytes)
  buffer.write('RIFF', 0); buffer.writeUInt32LE(36 + dataBytes, 4)
  buffer.write('WAVEfmt ', 8); buffer.writeUInt32LE(16, 16)
  buffer.writeUInt16LE(1, 20); buffer.writeUInt16LE(1, 22)
  buffer.writeUInt32LE(8_000, 24); buffer.writeUInt32LE(16_000, 28)
  buffer.writeUInt16LE(2, 32); buffer.writeUInt16LE(16, 34)
  buffer.write('data', 36); buffer.writeUInt32LE(dataBytes, 40)
  return buffer
}

for (const decision of [
  { button: 'Восстановить', expectedFilter: 'Сепия', label: 'recover' },
  { button: 'Открыть сохранённую', expectedFilter: 'Нет', label: 'discard' },
]) test(`renderer crash before debounce: ${decision.label} is durable`, async ({ browserName }) => {
  test.skip(browserName !== 'chromium', 'Renderer crash injection uses Chromium CDP')
  test.slow()
  const profile = await mkdtemp(join(tmpdir(), 'video-kadr-recovery-'))
  const baseURL = String(test.info().project.use.baseURL)
  let firstContext: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  let secondContext: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  try {
  firstContext = await chromium.launchPersistentContext(profile, { headless: true, baseURL })
  const page = firstContext.pages()[0] ?? await firstContext.newPage()
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'recovery.wav', mimeType: 'audio/wav', buffer: wavFixture(),
  })
  await expect(page.getByRole('heading', { name: 'recovery.wav' })).toBeVisible()
  await page.evaluate(() => {
    const nativeSetTimeout = window.setTimeout.bind(window)
    window.setTimeout = ((handler: TimerHandler, timeout?: number, ...args: unknown[]) =>
      nativeSetTimeout(handler, timeout === 1_000 ? 60_000 : timeout, ...args)) as typeof window.setTimeout
  })
  await page.getByRole('button', { name: 'Сепия' }).click()
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr', 2)
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const journals = database.transaction('project-journals').objectStore('project-journals').getAll()
    const values = await new Promise<Array<{ candidate?: { edit?: { filter?: string } } }>>((resolve, reject) => {
      journals.onsuccess = () => resolve(journals.result); journals.onerror = () => reject(journals.error)
    })
    database.close(); return values.some((item) => item.candidate?.edit?.filter === 'sepia')
  })).toBe(true)

  const crash = page.waitForEvent('crash')
  const cdp = await firstContext.newCDPSession(page)
  void cdp.send('Page.crash').catch(() => undefined)
  await crash
  await firstContext.close().catch(() => undefined)
  firstContext = undefined
  secondContext = await chromium.launchPersistentContext(profile, { headless: true, baseURL })
  const reopened = secondContext.pages()[0] ?? await secondContext.newPage()
  await reopened.goto('/?processing=browser')
  const item = reopened.locator('.lib-item').filter({ hasText: 'recovery.wav' })
  await item.getByRole('button', { name: 'Открыть как проект' }).click()
  const dialog = reopened.getByRole('alertdialog')
  await expect(dialog).toContainText('Восстановить несохранённые изменения')
  await dialog.getByRole('button', { name: decision.button }).click()
  await expect(dialog).toHaveCount(0)
  await expect(reopened.getByRole('button', { name: decision.expectedFilter, exact: true }).first()).toHaveClass(/active/)

  await reopened.reload({ waitUntil: 'networkidle' })
  await reopened.locator('.lib-item').filter({ hasText: 'recovery.wav' })
    .getByRole('button', { name: 'Открыть как проект' }).click()
  await expect(reopened.getByRole('alertdialog')).toHaveCount(0)
  } finally {
    await firstContext?.close()
    await secondContext?.close()
    await rm(profile, { recursive: true, force: true })
  }
})

test('corrupt mutable head remains visible and rolls back monotonically', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'IndexedDB corruption scenario is covered once in Chromium')
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'corrupt-head.wav', mimeType: 'audio/wav', buffer: wavFixture(),
  })
  await expect(page.getByRole('heading', { name: 'corrupt-head.wav' })).toBeVisible()
  await page.getByRole('button', { name: 'Сепия' }).click()
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr', 2)
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const values = database.transaction('projects').objectStore('projects').getAll()
    const projects = await new Promise<Array<{ revision?: number }>>((resolve, reject) => {
      values.onsuccess = () => resolve(values.result); values.onerror = () => reject(values.error)
    })
    database.close()
    return projects[0]?.revision ?? 0
  })).toBeGreaterThanOrEqual(1)
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr', 2)
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const values = database.transaction('project-journals').objectStore('project-journals').count()
    const count = await new Promise<number>((resolve, reject) => {
      values.onsuccess = () => resolve(values.result); values.onerror = () => reject(values.error)
    })
    database.close(); return count
  })).toBe(0)
  const originalRevision = await page.evaluate(async () => {
    const request = indexedDB.open('video-kadr', 2)
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const store = database.transaction('projects', 'readwrite').objectStore('projects')
    const values = store.getAll()
    const projects = await new Promise<Array<Record<string, unknown>>>((resolve, reject) => {
      values.onsuccess = () => resolve(values.result); values.onerror = () => reject(values.error)
    })
    const project = projects[0] as { revision: number; document: Record<string, unknown> }
    store.put({ ...project, document: { ...project.document, schemaVersion: 999 } })
    await new Promise<void>((resolve, reject) => {
      store.transaction.oncomplete = () => resolve(); store.transaction.onerror = () => reject(store.transaction.error)
    })
    database.close()
    return project.revision
  })
  await page.reload({ waitUntil: 'networkidle' })
  const item = page.locator('.lib-item').filter({ hasText: 'corrupt-head.wav' })
  await expect(item).toBeVisible()
  await item.getByRole('button', { name: 'Открыть как проект' }).click()
  const dialog = page.getByRole('alertdialog')
  await expect(dialog).toContainText('Последнее сохранение повреждено')
  await dialog.getByRole('button', { name: 'Откатить к исправной ревизии' }).click()
  await expect(dialog).toHaveCount(0)
  await expect.poll(() => page.evaluate(async () => {
    const request = indexedDB.open('video-kadr', 2)
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error)
    })
    const values = database.transaction('projects').objectStore('projects').getAll()
    const projects = await new Promise<Array<{ revision?: number }>>((resolve, reject) => {
      values.onsuccess = () => resolve(values.result); values.onerror = () => reject(values.error)
    })
    database.close(); return projects[0]?.revision ?? 0
  })).toBeGreaterThan(originalRevision)
})
