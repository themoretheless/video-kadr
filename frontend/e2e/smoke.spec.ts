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
    else if (path === '/api/composition-projects' && request.method() === 'GET') body = []
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

  await expect(page.getByRole('heading', { name: /Видеоредактор/ })).toBeVisible()
  await expect(page.getByRole('status')).toHaveText('Сервер недоступен')
  await expect(page.getByPlaceholder('https://vkvideo.ru/video-220018529_456248395')).toBeVisible()
})

test('multitrack workspace loads through its lazy production boundary', async ({ page }) => {
  await mockApi(page)
  await page.goto('/')

  await page.getByRole('button', { name: 'Multitrack' }).click()
  await expect(page.getByLabel('Название композиции')).toHaveValue('Новая композиция')
  await expect(page.getByRole('main')).toHaveClass(/composition-workspace/)
  await expect(page.getByText('Локальные шаблоны')).toBeVisible()
})

test('mocked import, LUT/curves edit and export workflow completes', async ({ page }) => {
  await mockApi(page)
  await page.goto('/')

  await page.getByPlaceholder('https://vkvideo.ru/video-220018529_456248395').fill('https://example.com/video')
  await page.getByRole('button', { name: 'Импорт' }).click()
  await expect(page.getByRole('heading', { name: 'Fixture clip' })).toBeVisible()

  const compareToggle = page.getByRole('button', { name: 'Оригинал / С правками' })
  await expect(compareToggle).toHaveAttribute('aria-pressed', 'false')
  await expect(page.getByText(/^С правками: монтаж/)).toBeVisible()

  await page.getByRole('button', { name: 'Сепия' }).click()
  const preview = page.locator('video.player').first()
  await page.getByRole('button', { name: '2×', exact: true }).click()
  await page.getByRole('slider', { name: 'Громкость' }).fill('0.4')
  await page.getByLabel('Без звука').check()
  await expect.poll(() => preview.evaluate((element) => element.style.filter)).toContain('sepia')
  await expect.poll(() => preview.evaluate((element) => element.playbackRate)).toBe(2)
  await expect.poll(() => preview.evaluate((element) => element.volume)).toBe(0.4)
  await expect.poll(() => preview.evaluate((element) => element.muted)).toBe(true)

  await compareToggle.click()
  await expect(compareToggle).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByText(/^Оригинал: монтаж/)).toBeVisible()
  await expect.poll(() => preview.evaluate((element) => element.style.filter)).toBe('')
  await expect.poll(() => preview.evaluate((element) => element.playbackRate)).toBe(1)
  await expect.poll(() => preview.evaluate((element) => element.volume)).toBe(1)
  await expect.poll(() => preview.evaluate((element) => element.muted)).toBe(false)

  await compareToggle.click()
  await expect(compareToggle).toHaveAttribute('aria-pressed', 'false')
  await expect.poll(() => preview.evaluate((element) => element.style.filter)).toContain('sepia')
  await expect.poll(() => preview.evaluate((element) => element.playbackRate)).toBe(2)
  await expect.poll(() => preview.evaluate((element) => element.volume)).toBe(0.4)
  await expect.poll(() => preview.evaluate((element) => element.muted)).toBe(true)

  await page.getByRole('button', { name: 'Активировать' }).click()
  await expect(page.locator('.timeline-segment')).toHaveCount(1)
  const timelineEnd = page.getByLabel('Конец, с')
  await timelineEnd.fill('6')
  await timelineEnd.press('Tab')
  await preview.evaluate((element) => {
    element.currentTime = 3
    element.dispatchEvent(new Event('timeupdate'))
  })
  await page.getByRole('button', { name: 'Разделить здесь' }).click()
  await expect(page.locator('.timeline-segment')).toHaveCount(2)

  await page.getByLabel('Конец, с').fill('10')
  await page.getByLabel('Конец, с').press('Tab')
  await page.getByLabel('Начало, с').fill('8')
  await page.getByLabel('Начало, с').press('Tab')
  await page.getByRole('button', { name: 'Переместить выбранный фрагмент левее' }).click()
  await page.getByRole('button', { name: 'Дубль' }).click()
  await page.getByRole('button', { name: 'Удалить', exact: true }).click()
  await page.getByRole('button', { name: 'Дубль' }).click()
  await expect(page.locator('.timeline-segment')).toHaveCount(3)

  await compareToggle.click()
  await preview.evaluate((element) => {
    element.currentTime = 4
    element.dispatchEvent(new Event('timeupdate'))
  })
  await expect.poll(() => preview.evaluate((element) => element.currentTime)).toBe(4)
  await compareToggle.click()
  await expect.poll(() => preview.evaluate((element) => element.currentTime)).toBe(0)

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

  const curvePlot = page.getByRole('group', { name: 'Общая тоновая кривая' })
  const pointCount = page.locator('.point-count')
  const addPoint = page.getByRole('button', { name: 'Добавить точку' })
  const deletePoint = page.getByRole('button', { name: 'Удалить точку' })
  const curveX = page.getByLabel('Вход (X)')
  const curveY = page.getByLabel('Выход (Y)')

  await expect(pointCount).toHaveText('2 / 16 точек')
  const plotBox = await curvePlot.boundingBox()
  if (!plotBox) throw new Error('Curve plot has no bounding box')

  // Clicking the graph adds a point and immediately starts a bounded drag.
  await page.mouse.click(
    plotBox.x + plotBox.width * 0.4,
    plotBox.y + plotBox.height * 0.3,
  )
  await expect(pointCount).toHaveText('3 / 16 точек')
  await expect(curveX).toBeEnabled()
  await expect(deletePoint).toBeEnabled()

  const beforeX = Number(await curveX.inputValue())
  const beforeY = Number(await curveY.inputValue())
  const middlePoint = () => curvePlot.getByRole('button', { name: /^Общая: точка 2,/ })
  const pointBox = await middlePoint().boundingBox()
  if (!pointBox) throw new Error('Middle curve point has no bounding box')
  const pointX = pointBox.x + pointBox.width / 2
  const pointY = pointBox.y + pointBox.height / 2

  await page.mouse.move(pointX, pointY)
  await page.mouse.down()
  try {
    await page.mouse.move(
      pointX + plotBox.width * 0.12,
      pointY + plotBox.height * 0.08,
      { steps: 3 },
    )
  } finally {
    await page.mouse.up()
  }
  await expect.poll(async () => Number(await curveX.inputValue())).toBeGreaterThan(beforeX + 20)
  await expect.poll(async () => Number(await curveY.inputValue())).toBeLessThan(beforeY - 10)

  await middlePoint().focus()
  await middlePoint().press('Delete')
  await expect(pointCount).toHaveText('2 / 16 точек')
  await expect(deletePoint).toBeDisabled()
  await expect(curveX).toBeDisabled()

  // Keep the final payload assertion below at three points.
  await addPoint.click()
  await expect(pointCount).toHaveText('3 / 16 точек')

  const editRequestPromise = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === '/api/edit' && request.method() === 'POST',
  )
  await page.getByRole('button', { name: 'Экспортировать' }).click()
  const editPayload = (await editRequestPromise).postDataJSON() as {
    lut?: { id: string; intensity: number }
    curves?: Record<string, Array<{ x: number; y: number }>>
    segments?: Array<{ start: number; end: number }>
  }
  expect(editPayload.segments).toEqual([
    { start: 8, end: 10 },
    { start: 0, end: 3 },
    { start: 0, end: 3 },
  ])
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
