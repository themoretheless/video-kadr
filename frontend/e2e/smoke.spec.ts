import { expect, test, type Page, type Route } from '@playwright/test'

const TEST_LUT_ID = '11111111-1111-4111-8111-111111111111'

const capabilities = {
  schemaVersion: 1,
  toolFingerprint: 'e2e-fixture',
  formats: ['mp4', 'webm', 'av1', 'prores', 'gif', 'png', 'jpg', 'mp3'].map((id) => ({
    id,
    label: id,
    available: true,
  })),
  codecs: ['h264', 'h265'].map((id) => ({ id, label: id, available: true })),
  filters: [
    'grayscale',
    'sepia',
    'warm',
    'cold',
    'teal-orange',
    'faded',
    'noir',
    'vintage',
    'custom-curves',
    'lut3d',
    'lut-intensity',
  ].map((id) => ({ id, label: id, available: true })),
  hardware: [],
}

async function mockApi(page: Page): Promise<void> {
  await page.route('**/files/**', async (route) => {
    await route.fulfill({ status: 204, contentType: 'video/mp4', body: '' })
  })
  await page.route('**/api/**', async (route: Route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    let status = 200
    let body: unknown = {}

    if (path === '/api/capabilities') body = capabilities
    else if (path === '/api/library') body = []
    else if (path === '/api/import' && request.method() === 'POST') body = { jobId: 'import-1' }
    else if (path === '/api/luts' && request.method() === 'POST') {
      body = {
        schemaVersion: 1,
        id: TEST_LUT_ID,
        name: 'Fixture LUT',
        kind: 'cube3d',
        cubeSize: 2,
        sizeBytes: 164,
        sha256: 'fixture-sha256',
        createdAt: 1,
      }
    }
    else if (path === '/api/jobs/import-1') {
      body = {
        id: 'import-1',
        status: 'done',
        result: {
          id: 'clip-1',
          url: '/files/sources/clip-1.mp4',
          filename: 'clip-1.mp4',
          duration: 12,
          width: 1280,
          height: 720,
          title: 'Fixture clip',
        },
      }
    } else if (path === '/api/edit' && request.method() === 'POST') body = { jobId: 'export-1' }
    else if (path === '/api/jobs/export-1') {
      body = {
        id: 'export-1',
        status: 'done',
        result: { id: 'result-1', url: '/files/outputs/result-1.mp4', filename: 'result-1.mp4' },
      }
    } else if (path.startsWith('/api/projects/by-video/')) {
      status = 404
      body = { error: 'Проект не найден', code: 'not_found' }
    } else if (path === '/api/projects' && request.method() === 'POST') {
      body = { id: 'project-1' }
    } else {
      status = 404
      body = { error: `Unhandled mock route: ${request.method()} ${path}`, code: 'not_found' }
    }

    await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
  })
}

async function mockOffline(page: Page): Promise<void> {
  await page.route('**/api/**', (route) =>
    route.fulfill({
      status: 503,
      contentType: 'application/json',
      body: JSON.stringify({ error: 'backend unavailable' }),
    }),
  )
}

test('editor shell opens and explains that the backend is offline', async ({ page }) => {
  await mockOffline(page)
  await page.goto('/')

  await expect(page.getByRole('heading', { name: /Video Kadr/ })).toBeVisible()
  await expect(page.getByRole('status')).toHaveText('Сервер недоступен')
  await expect(page.getByPlaceholder('https://vkvideo.ru/video-220018529_456248395')).toBeVisible()
})

test('mocked import, LUT/curves edit and export workflow completes', async ({ page }) => {
  await mockApi(page)
  await page.goto('/')

  await page.getByPlaceholder('https://vkvideo.ru/video-220018529_456248395').fill('https://example.com/video')
  await page.getByRole('button', { name: 'Импорт' }).click()
  await expect(page.getByRole('heading', { name: 'Fixture clip' })).toBeVisible()

  await page.getByLabel('Выбрать LUT в формате CUBE').setInputFiles({
    name: 'fixture-look.cube',
    mimeType: 'text/plain',
    buffer: Buffer.from(
      [
        'TITLE "Fixture LUT"',
        'LUT_3D_SIZE 2',
        '0 0 0',
        '1 0 0',
        '0 1 0',
        '1 1 0',
        '0 0 1',
        '1 0 1',
        '0 1 1',
        '1 1 1',
      ].join('\n'),
    ),
  })
  await expect(page.getByText('Fixture LUT', { exact: true })).toBeVisible()
  await expect(page.getByText('Таблица 2×2×2')).toBeVisible()

  const lutIntensity = page.getByRole('slider', { name: 'Интенсивность' })
  await lutIntensity.fill('65')
  await expect(page.locator('output[for="lut-intensity"]')).toHaveText('65%')

  await page.getByRole('button', { name: 'Добавить точку' }).click()
  await expect(page.getByText('3 / 16 точек')).toBeVisible()

  await page.getByRole('button', { name: 'Сепия' }).click()
  const editRequestPromise = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === '/api/edit' && request.method() === 'POST',
  )
  await page.getByRole('button', { name: 'Экспортировать', exact: true }).click()
  const editPayload = (await editRequestPromise).postDataJSON() as {
    lut?: { id: string; intensity: number }
    curves?: Record<string, Array<{ x: number; y: number }>>
  }
  expect(editPayload.lut).toEqual({ id: TEST_LUT_ID, intensity: 0.65 })
  expect(Object.keys(editPayload.curves ?? {}).sort()).toEqual(['blue', 'green', 'master', 'red'])
  expect(editPayload.curves?.master).toHaveLength(3)
  expect(editPayload.curves?.master[0]).toEqual({ x: 0, y: 0 })
  expect(editPayload.curves?.master[2]).toEqual({ x: 1, y: 1 })
  await expect(page.getByRole('link', { name: 'Скачать результат' })).toBeVisible()
})

test('390px viewport has no horizontal page overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await mockOffline(page)
  await page.goto('/')
  await expect(page.getByRole('status')).toHaveText('Сервер недоступен')

  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    content: document.documentElement.scrollWidth,
  }))
  expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport)
})
