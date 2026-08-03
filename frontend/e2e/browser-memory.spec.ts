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

test('bounded browser export succeeds twice without reloading', async ({ page, browserName }) => {
  test.skip(browserName === 'firefox' && process.platform === 'darwin', 'The packaged Firefox runner aborts ffmpeg.wasm on macOS; Linux CI keeps this gate enabled')
  test.setTimeout(120_000)
  await page.goto('/?processing=browser')
  const file = { name: 'memory-small.wav', mimeType: 'audio/wav', buffer: wavFixture(8_000) }
  await page.locator('.dropzone input[type=file]').setInputFiles(file)
  await expect(page.getByText('Ресурсы локального экспорта')).toBeVisible()
  await expect(page.locator('.resource-plan')).toContainText(browserName === 'firefox' ? 'MEMFS' : 'WORKERFS')
  if (browserName !== 'firefox') {
    await page.evaluate(() => {
      const original = File.prototype.arrayBuffer
      File.prototype.arrayBuffer = function () {
        if (this.name === 'memory-small.wav') throw new Error('full source arrayBuffer forbidden by WORKERFS gate')
        return original.call(this)
      }
    })
  }
  await page.getByRole('button', { name: 'MP3' }).click()
  const run = async () => {
    await page.getByRole('button', { name: 'Экспортировать' }).click()
    const copy = page.getByRole('button', { name: 'Создать копию' })
    if (await copy.isVisible()) await copy.click()
    await expect(page.getByRole('link', { name: 'Скачать результат' })).toBeVisible({ timeout: 60_000 })
  }
  await run()
  await run()
  await expect(page.getByRole('heading', { name: 'memory-small.wav' })).toBeVisible()
})

test('unsafe 4K reverse is blocked in the editor before a browser job is created', async ({ page }) => {
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'planner.wav', mimeType: 'audio/wav', buffer: wavFixture(8_000),
  })
  await expect(page.getByRole('heading', { name: 'planner.wav' })).toBeVisible()
  await page.evaluate(async () => {
    const store = await import('/src/store.ts')
    Object.assign(store.state.video!, {
      duration: 600, width: 3840, height: 2160, fps: 30, sizeBytes: 1024 * 1024 * 1024,
      mediaKind: 'video',
    })
    Object.assign(store.state.edit, { reverse: true, trimStart: 0, trimEnd: 600 })
  })
  const plan = page.locator('.resource-plan')
  await expect(plan).toHaveAttribute('role', 'alert')
  await expect(plan).toContainText(/Сократите диапазон/)
  await expect(page.getByRole('button', { name: 'Экспортировать' })).toBeDisabled()
  expect(await page.evaluate(async () => (await import('/src/store.ts')).state.exportJobId)).toBeNull()

  await page.evaluate(async () => {
    const store = await import('/src/store.ts')
    Object.assign(store.state.edit, { trimEnd: 0.1, scaleEnabled: true, scale: { w: 320, h: -2 } })
  })
  await expect(page.getByRole('button', { name: 'Экспортировать' })).toBeEnabled()
})

test('low-memory mobile viewport keeps blocked-plan actions readable and idle', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, 'deviceMemory', { configurable: true, value: 0.5 }))
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'mobile.wav', mimeType: 'audio/wav', buffer: wavFixture(8_000),
  })
  const alert = page.locator('.resource-plan[role=alert]')
  await expect(alert).toBeVisible()
  await expect(alert).toContainText(/полноценную серверную версию/)
  await expect(page.getByRole('button', { name: 'Экспортировать' })).toBeDisabled()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true)
})

test('Chromium streams recorder chunks directly to a chosen file handle', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'File System Access streaming tier is Chromium-only')
  await page.addInitScript(() => {
    const chunks: Blob[] = []
    ;(window as Window & { __streamChunks?: Blob[]; __streamAborted?: boolean }).__streamChunks = chunks
    Object.defineProperty(window, 'showSaveFilePicker', {
      configurable: true,
      value: async () => ({
        name: 'streamed.webm',
        createWritable: async () => ({
          write: async (chunk: Blob) => { chunks.push(chunk) },
          close: async () => undefined,
          abort: async () => { (window as Window & { __streamAborted?: boolean }).__streamAborted = true },
        }),
        getFile: async () => new File(chunks, 'streamed.webm', { type: 'video/webm' }),
      }),
    })
  })
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'stream-source.wav', mimeType: 'audio/wav', buffer: wavFixture(8_000),
  })
  const button = page.getByRole('button', { name: 'Потоково сохранить оригинал' })
  await expect(button).toBeVisible()
  await button.click()
  await expect(page.getByRole('link', { name: 'Скачать результат' })).toBeVisible({ timeout: 10_000 })
  const chunks = await page.evaluate(() => (window as Window & { __streamChunks: Blob[] }).__streamChunks.map((chunk) => chunk.size))
  expect(chunks.length).toBeGreaterThan(0)
  expect(Math.max(...chunks)).toBeLessThan(1024 * 1024)

  await button.click()
  await page.waitForTimeout(200)
  await page.getByRole('button', { name: 'Отмена' }).click()
  await expect(page.getByRole('heading', { name: 'Результат' })).toHaveCount(0)
  expect(await page.evaluate(() => (window as Window & { __streamAborted?: boolean }).__streamAborted)).toBe(true)
})
