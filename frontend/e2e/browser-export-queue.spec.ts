import { expect, test } from '@playwright/test'

function wavFixture(samples = 4_000): Buffer {
  const dataBytes = samples * 2
  const buffer = Buffer.alloc(44 + dataBytes)
  buffer.write('RIFF', 0); buffer.writeUInt32LE(36 + dataBytes, 4); buffer.write('WAVEfmt ', 8)
  buffer.writeUInt32LE(16, 16); buffer.writeUInt16LE(1, 20); buffer.writeUInt16LE(1, 22)
  buffer.writeUInt32LE(8_000, 24); buffer.writeUInt32LE(16_000, 28); buffer.writeUInt16LE(2, 32); buffer.writeUInt16LE(16, 34)
  buffer.write('data', 36); buffer.writeUInt32LE(dataBytes, 40)
  return buffer
}

async function importTinySource(page: import('@playwright/test').Page, samples = 4_000): Promise<void> {
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'queue-small.wav', mimeType: 'audio/wav', buffer: wavFixture(samples),
  })
  await expect(page.getByRole('heading', { name: 'queue-small.wav' })).toBeVisible()
}

test('static batch queue renders two variants strictly sequentially with distinct session downloads', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  test.setTimeout(120_000)
  await importTinySource(page)
  await page.getByRole('button', { name: 'Пакетный экспорт' }).click()
  const names = page.locator('input[aria-label^="Название варианта"]')
  await names.nth(0).fill('Audio compact')
  await names.nth(1).fill('Audio archive')
  const formats = page.locator('select[aria-label^="Формат варианта"]')
  await formats.nth(0).selectOption('mp3')
  await formats.nth(1).selectOption('mp3')
  await page.evaluate(async () => {
    const center = await import('/src/export-queue-center.ts')
    const probe = { maxRunning: 0, doneOrder: [] as string[] }
    ;(window as typeof window & { __exportQueueProbe?: typeof probe }).__exportQueueProbe = probe
    const seen = new Set<string>()
    const timer = window.setInterval(() => {
      probe.maxRunning = Math.max(probe.maxRunning, center.exportQueueState.tasks.filter(task => task.status === 'running').length)
      for (const task of center.exportQueueState.tasks) if (task.status === 'done' && !seen.has(task.id)) { seen.add(task.id); probe.doneOrder.push(task.name) }
      if (probe.doneOrder.length === 2) window.clearInterval(timer)
    }, 5)
  })
  await page.getByRole('button', { name: 'Поставить пакет в очередь' }).click()

  const panel = page.getByRole('region', { name: 'Очередь экспорта' })
  await expect(panel).toContainText('Audio compact')
  await expect(panel).toContainText('Audio archive')
  const first = page.getByRole('link', { name: /Скачать Audio compact/ })
  const second = page.getByRole('link', { name: /Скачать Audio archive/ })
  await expect(first).toBeVisible({ timeout: 60_000 })
  await expect(second).toBeVisible({ timeout: 60_000 })
  expect(await first.getAttribute('href')).not.toBe(await second.getAttribute('href'))
  const history = await page.evaluate(async () => {
    const center = await import('/src/export-queue-center.ts')
    return center.exportQueueState.tasks.map(task => ({ name: task.name, status: task.status, progress: task.progress }))
  })
  expect(history).toEqual([
    expect.objectContaining({ name: 'Audio compact', status: 'done', progress: 100 }),
    expect.objectContaining({ name: 'Audio archive', status: 'done', progress: 100 }),
  ])
  expect(await page.evaluate(() => (window as typeof window & { __exportQueueProbe: { maxRunning: number; doneOrder: string[] } }).__exportQueueProbe)).toEqual({
    maxRunning: 1, doneOrder: ['Audio compact', 'Audio archive'],
  })
})

test('reload automatically restarts an expired WASM row from zero and exposes the attempt', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  await importTinySource(page)
  await page.evaluate(async () => {
    const store = await import('/src/store.ts')
    const queueModule = await import('/src/browser-export-queue.ts')
    const video = store.state.video!
    const queue = new queueModule.BrowserExportQueue()
    const source = { assetRef: video.assetId ?? video.id, fingerprint: video.fingerprint! }
    await queue.enqueue([{
      contract: 'export-batch-v1', id: 'reload-batch:reload-variant', batchId: 'reload-batch',
      variantId: 'reload-variant', ordinal: 0, label: 'Reload variant',
      source, dependencies: [{ kind: 'source', ...source }],
      payload: { videoId: video.id, format: 'mp3' },
    }])
    await queue.claim(1)
    await queue.close()
  })
  await page.waitForTimeout(10)
  await page.reload({ waitUntil: 'networkidle' })
  const panel = page.getByRole('region', { name: 'Очередь экспорта' })
  await expect(panel).toContainText('Reload variant')
  await expect(panel).toContainText('Перезапуск с 0% · попытка 2')
  await expect(page.getByRole('link', { name: /Скачать Reload variant/ })).toBeVisible({ timeout: 60_000 })
  await expect(page.getByRole('button', { name: 'Начать заново Reload variant' })).toHaveCount(0)
})

test('cancelling queued and running variants releases the next sequential render', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  test.setTimeout(120_000)
  await importTinySource(page)
  await page.getByRole('button', { name: 'Пакетный экспорт' }).click()
  await page.getByRole('button', { name: 'Добавить вариант' }).click()
  const labels = ['Cancel running', 'Cancel queued', 'Must finish']
  const names = page.locator('input[aria-label^="Название варианта"]')
  const formats = page.locator('select[aria-label^="Формат варианта"]')
  for (let index = 0; index < labels.length; index++) {
    await names.nth(index).fill(labels[index]!)
    await formats.nth(index).selectOption('mp3')
  }
  await page.getByRole('button', { name: 'Поставить пакет в очередь' }).click()
  const panel = page.getByRole('region', { name: 'Очередь экспорта' })
  const row = (name: string) => panel.getByRole('listitem').filter({ hasText: name })
  await expect(row('Cancel running')).toContainText('Выполняется', { timeout: 15_000 })
  await row('Cancel queued').getByRole('button', { name: 'Отменить Cancel queued' }).click()
  await row('Cancel running').getByRole('button', { name: 'Отменить Cancel running' }).click()
  await expect(row('Cancel queued')).toContainText('Отменено')
  await expect(row('Cancel running')).toContainText('Отменено')
  await expect(page.getByRole('link', { name: /Скачать Must finish/ })).toBeVisible({ timeout: 60_000 })
})

test('permission loss distinguishes source and LUT recovery with retry and cancel actions', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  await importTinySource(page)
  await page.evaluate(async () => {
    const store = await import('/src/store.ts')
    const queueModule = await import('/src/browser-export-queue.ts')
    const video = store.state.video!
    const queue = new queueModule.BrowserExportQueue()
    const source = { assetRef: video.assetId ?? video.id, fingerprint: video.fingerprint! }
    await queue.enqueue([{
      contract: 'export-batch-v1', id: 'permission-batch:permission-variant', batchId: 'permission-batch',
      variantId: 'permission-variant', ordinal: 0, label: 'Permission variant',
      source, dependencies: [{ kind: 'source', ...source }],
      payload: { videoId: video.id, format: 'mp3' },
    }])
    const claimed = await queue.claim()
    await queue.requirePermission(claimed!.definition.id, claimed!.generation, 'Непрозрачная ошибка A-17 с текстом LUT', 'source')
    await queue.enqueue([{
      contract: 'export-batch-v1', id: 'lut-permission-batch:lut-permission-variant', batchId: 'lut-permission-batch',
      variantId: 'lut-permission-variant', ordinal: 0, label: 'LUT permission variant',
      source, dependencies: [{ kind: 'source', ...source }, { kind: 'lut', assetRef: 'cinema-lut', fingerprint: 'a'.repeat(64) }],
      payload: { videoId: video.id, format: 'mp3' },
    }])
    const lutClaimed = await queue.claim()
    await queue.requirePermission(lutClaimed!.definition.id, lutClaimed!.generation, 'Непрозрачная ошибка B-29', 'lut')
    await queue.close()
  })
  await page.reload({ waitUntil: 'networkidle' })
  const panel = page.getByRole('region', { name: 'Очередь экспорта' })
  await expect(panel).toContainText('Permission variant')
  await expect(panel).toContainText('Нужно разрешение')
  await expect(page.getByRole('link', { name: 'Перейти в медиатеку для перепривязки Permission variant' })).toHaveAttribute('href', '#media-library')
  await expect(page.getByRole('button', { name: 'Повторить после перепривязки Permission variant' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Отменить Permission variant' })).toBeVisible()
  await expect(panel).toContainText('LUT permission variant')
  await expect(panel).toContainText('повторно загрузите файл .cube')
  await expect(page.getByRole('link', { name: 'Перейти в библиотеку LUT для восстановления LUT permission variant' })).toHaveAttribute('href', '#lut-library')
  await expect(page.getByRole('button', { name: 'Повторить после перепривязки LUT permission variant' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Отменить LUT permission variant' })).toBeVisible()
})

test('target-size UI validates accessibly and persists an immutable per-variant contract', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  await importTinySource(page, 80_000)
  await page.getByRole('radio', { name: 'В размер', exact: true }).click()
  const target = page.getByLabel('Целевой размер, МБ', { exact: true })
  await target.fill('0')
  await expect(target).toHaveAttribute('aria-invalid', 'true')
  await expect(page.getByRole('alert')).toContainText('не меньше 1 МБ')
  await expect(page.getByRole('button', { name: 'Экспортировать', exact: true })).toBeDisabled()
  await target.fill('5')
  await expect(target).toHaveAttribute('aria-invalid', 'false')
  await expect(page.getByRole('status').filter({ hasText: 'Оценка размера' })).toContainText('Видеобитрейт')
  // This fixture is audio-only, so the runtime capability probe disables
  // video formats. Enable the supported contract explicitly for the queue
  // snapshot assertion; unsupported UI behavior is covered separately.
  await page.evaluate(async () => {
    const store = await import('/src/store.ts')
    for (const format of store.state.capabilities?.formats ?? []) if (format.id === 'mp4') format.available = true
    for (const codec of store.state.capabilities?.codecs ?? []) if (codec.id === 'h264') codec.available = true
  })
  await page.getByRole('button', { name: 'Пакетный экспорт' }).click()
  const firstRateControl = page.getByRole('group', { name: 'Управление размером варианта 1' })
  await firstRateControl.getByRole('radio', { name: 'В размер' }).click()
  const variantTarget = page.getByLabel('Целевой размер варианта 1, МБ')
  await variantTarget.fill('0')
  await expect(variantTarget).toHaveAttribute('aria-invalid', 'true')
  await expect(variantTarget).toHaveAttribute('aria-describedby', /^variant-target-error-/)
  await variantTarget.fill('5')
  const names = page.locator('input[aria-label^="Название варианта"]')
  await names.nth(0).fill('Target snapshot')
  await names.nth(1).fill('Quality snapshot')
  await page.getByRole('button', { name: 'Поставить пакет в очередь' }).click()
  await expect(page.getByRole('region', { name: 'Очередь экспорта' })).toContainText('Target snapshot')
  const stored = await page.evaluate(async () => {
    const queueModule = await import('/src/browser-export-queue.ts')
    const queue = new queueModule.BrowserExportQueue()
    const jobs = (await queue.list()).map(item => ({ label: item.definition.label, rateControl: item.definition.payload.rateControl, hasLegacyQuality: Object.hasOwn(item.definition.payload, 'quality') }))
    await queue.close()
    return jobs
  })
  expect(stored.find(item => item.label === 'Target snapshot')?.rateControl, JSON.stringify(stored)).toMatchObject({ mode: 'target_size', targetBytes: 5_000_000, estimatorVersion: 'size-v1' })
  expect(stored.find(item => item.label === 'Target snapshot')?.hasLegacyQuality).toBe(false)
})

test('unsupported target formats and codecs expose reasons and cannot enqueue', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'The required static queue release gate is Chromium')
  await importTinySource(page)
  const prores = page.getByRole('button', { name: /ProRes\./ })
  await expect(prores).toBeDisabled()
  await expect(prores).toHaveAttribute('aria-label', /Недоступно/)
  const h265 = page.getByRole('button', { name: /H\.265\./ })
  await expect(h265).toBeDisabled()
  await expect(h265).toHaveAttribute('aria-label', /Недоступно/)
  await page.getByRole('button', { name: 'Аудио MP3' }).click()
  await expect(page.getByText('Целевой размер недоступен для этого формата.', { exact: true })).toBeVisible()
  await expect(page.getByRole('radio', { name: 'В размер', exact: true })).toHaveCount(0)
  const count = await page.evaluate(async () => {
    const queueModule = await import('/src/browser-export-queue.ts')
    const queue = new queueModule.BrowserExportQueue(); const jobs = await queue.list(); await queue.close(); return jobs.length
  })
  expect(count).toBe(0)
})
