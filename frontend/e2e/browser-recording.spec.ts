import { expect, test } from '@playwright/test'

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    ;(window as unknown as { __captureCalls: { display: number; user: number } }).__captureCalls = { display: 0, user: 0 }
    class FakeRecorder {
      static isTypeSupported() { return true }
    }
    Object.defineProperty(window, 'MediaRecorder', { configurable: true, value: FakeRecorder })
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
      getDisplayMedia: async () => {
        ;(window as unknown as { __captureCalls: { display: number } }).__captureCalls.display++
        throw new DOMException('denied', 'NotAllowedError')
      },
      getUserMedia: async () => {
        ;(window as unknown as { __captureCalls: { user: number } }).__captureCalls.user++
        throw new DOMException('denied', 'NotAllowedError')
      },
    } })
  })
  await page.goto('/?processing=browser')
})

test('recorder asks for permission only after a user gesture and reports the exact denial', async ({ page }) => {
  await expect(page.getByRole('heading', { name: 'Запись экрана и камеры' })).toBeVisible()
  await expect.poll(() => page.evaluate(() => (window as unknown as { __captureCalls: { display: number; user: number } }).__captureCalls)).toEqual({ display: 0, user: 0 })
  await page.getByRole('button', { name: 'Начать запись' }).click()
  await expect(page.getByRole('alert')).toContainText('Доступ к экрану не разрешён')
  await expect.poll(() => page.evaluate(() => (window as unknown as { __captureCalls: { display: number; user: number } }).__captureCalls)).toEqual({ display: 1, user: 0 })
})

test('recorder requires at least one visual source', async ({ page }) => {
  await page.getByLabel('Экран', { exact: true }).uncheck()
  await expect(page.getByText('Выберите экран или камеру.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Начать запись' })).toBeDisabled()
})
