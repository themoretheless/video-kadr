import { expect, test } from '@playwright/test'

test('screen camera and microphone recording is saved and inserted once', async ({ page }) => {
  await page.addInitScript(() => {
    const keepAlive: unknown[] = []
    const videoStream = (color: string) => {
      const canvas = document.createElement('canvas'); canvas.width = 320; canvas.height = 180
      const context = canvas.getContext('2d')!; context.fillStyle = color; context.fillRect(0, 0, 320, 180)
      keepAlive.push(canvas)
      return canvas.captureStream(15)
    }
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
      getDisplayMedia: async () => videoStream('#2463eb'),
      getUserMedia: async (constraints: MediaStreamConstraints) => {
        if (constraints.video) return videoStream('#ef4444')
        const audio = new AudioContext(); const oscillator = audio.createOscillator(); const destination = audio.createMediaStreamDestination()
        oscillator.connect(destination); oscillator.start(); keepAlive.push(audio, oscillator)
        return destination.stream
      },
    } })
    ;(window as unknown as { __recordingKeepAlive: unknown[] }).__recordingKeepAlive = keepAlive
  })
  await page.goto('/?processing=browser')
  await page.getByLabel('Камера', { exact: true }).check()
  await page.getByRole('button', { name: 'Начать запись' }).click()
  await expect(page.getByRole('button', { name: 'Пауза' })).toBeVisible()
  await page.waitForTimeout(1_100)
  await page.getByRole('button', { name: 'Пауза' }).click()
  await expect(page.getByRole('button', { name: 'Продолжить' })).toHaveAttribute('aria-pressed', 'true')
  await page.getByRole('button', { name: 'Продолжить' }).click()
  await page.waitForTimeout(250)
  await page.getByRole('button', { name: 'Завершить и сохранить' }).click()
  await expect(page.getByText(/recording-.*\.webm.*сохранена в медиатеке/)).toBeVisible({ timeout: 20_000 })
  await expect(page.locator('.timeline-editor .timeline-clip')).toHaveCount(1)
  await expect(page.locator('#media-library .lib-name').filter({ hasText: /recording-.*\.webm/ })).toBeVisible()
})
