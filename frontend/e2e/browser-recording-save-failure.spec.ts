import { expect, test } from '@playwright/test'

test('failed recording ingest can be discarded and a new recording can start', async ({ page }) => {
  await page.addInitScript(() => {
    class CorruptRecorder {
      static isTypeSupported() { return true }
      state: RecordingState = 'inactive'
      ondataavailable: ((event: { data: Blob }) => void) | null = null
      onstop: (() => void) | null = null
      onerror: (() => void) | null = null
      start() { this.state = 'recording' }
      pause() { this.state = 'paused' }
      resume() { this.state = 'recording' }
      stop() { this.ondataavailable?.({ data: new Blob(['not-webm'], { type: 'video/webm' }) }); this.state = 'inactive'; this.onstop?.() }
    }
    Object.defineProperty(window, 'MediaRecorder', { configurable: true, value: CorruptRecorder })
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
      getDisplayMedia: async () => {
        const canvas = document.createElement('canvas'); canvas.width = 160; canvas.height = 90
        canvas.getContext('2d')!.fillRect(0, 0, 160, 90)
        return canvas.captureStream(15)
      },
      getUserMedia: async () => new MediaStream(),
    } })
  })
  await page.goto('/?processing=browser')
  await page.getByLabel('Микрофон', { exact: true }).uncheck()
  await page.getByLabel(/\u0417\u0432\u0443\u043a \u0432\u043a\u043b\u0430\u0434\u043a\u0438/).uncheck()
  await page.getByRole('button', { name: 'Начать запись' }).click()
  await expect(page.getByRole('button', { name: 'Завершить и сохранить' })).toBeVisible()
  await page.getByRole('button', { name: 'Завершить и сохранить' }).click()
  await expect(page.getByRole('button', { name: 'Повторить сохранение' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Начать запись' })).toBeDisabled()
  await page.getByRole('button', { name: 'Удалить несохранённую запись' }).click()
  await page.getByRole('button', { name: 'Удалить запись', exact: true }).click()
  await expect(page.getByRole('button', { name: 'Начать запись' })).toBeEnabled()
  await expect(page.locator('#media-library .lib-item')).toHaveCount(0)
})
