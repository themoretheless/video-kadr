import { execFile } from 'node:child_process'
import { stat } from 'node:fs/promises'
import { promisify } from 'node:util'
import { expect, test } from '@playwright/test'

const execFileAsync = promisify(execFile)

interface JobSnapshot {
  status?: string
  result?: { url?: string; filename?: string }
}

interface ProbeOutput {
  format?: { duration?: string }
  streams?: Array<{
    codec_type?: string
    codec_name?: string
    width?: number
    height?: number
    avg_frame_rate?: string
  }>
}

test('real UI upload renders a composition and downloads a probeable file', async ({ page, request }) => {
  const fixturePath = process.env.REAL_COMPOSITION_FIXTURE
  if (!fixturePath) throw new Error('REAL_COMPOSITION_FIXTURE is required')

  const apiRequests: string[] = []
  page.on('request', (current) => {
    const url = new URL(current.url())
    if (url.pathname.startsWith('/api/')) apiRequests.push(`${current.method()} ${url.pathname}`)
  })

  await page.goto('/')
  await expect(page.getByRole('heading', { name: /Видеоредактор/ })).toBeVisible()
  await page.getByRole('button', { name: 'Multitrack' }).click()
  await expect(page.getByLabel('Название композиции')).toHaveValue('Новая композиция')

  await page.locator('.dropzone input[type="file"]').setInputFiles(fixturePath)
  await expect(page.locator('.composition-clip.video')).toHaveCount(1)
  await expect(page.locator('.lib-name', { hasText: 'real-composition-fixture' })).toBeVisible()

  const exportButton = page.getByRole('button', { name: 'Экспорт .mp4' })
  await expect(exportButton).toBeEnabled()
  const renderResponsePromise = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/compositions/render'
      && response.request().method() === 'POST',
  )
  await exportButton.click()
  const renderResponse = await renderResponsePromise
  expect(renderResponse.status()).toBe(200)
  const { jobId } = await renderResponse.json() as { jobId: string }
  expect(jobId).toMatch(/^[0-9a-f-]{36}$/)

  const downloadLink = page.getByRole('link', { name: /^Скачать .*\.mp4$/ })
  await expect(downloadLink).toBeVisible({ timeout: 60_000 })

  const jobResponse = await request.get(`/api/jobs/${encodeURIComponent(jobId)}`)
  expect(jobResponse.ok()).toBe(true)
  const terminal = await jobResponse.json() as JobSnapshot
  expect(terminal.status).toBe('done')
  expect(terminal.result?.url).toMatch(/^\/files\/outputs\/[0-9a-f-]{36}\.mp4$/)
  expect(terminal.result?.filename).toMatch(/^[0-9a-f-]{36}\.mp4$/)

  const downloadPromise = page.waitForEvent('download')
  await downloadLink.click()
  const download = await downloadPromise
  expect(download.suggestedFilename()).toBe(terminal.result?.filename)
  const downloadedPath = await download.path()
  if (!downloadedPath) throw new Error('Playwright did not retain the downloaded composition')
  expect((await stat(downloadedPath)).size).toBeGreaterThan(1_000)

  const { stdout } = await execFileAsync(
    'ffprobe',
    ['-v', 'error', '-show_entries', 'format=duration:stream=codec_type,codec_name,width,height,avg_frame_rate', '-of', 'json', downloadedPath],
    { timeout: 15_000, maxBuffer: 1024 * 1024 },
  )
  const probe = JSON.parse(stdout) as ProbeOutput
  const video = probe.streams?.find((stream) => stream.codec_type === 'video')
  const audio = probe.streams?.find((stream) => stream.codec_type === 'audio')
  expect(video).toMatchObject({ codec_name: 'h264', width: 160, height: 90 })
  expect(video?.avg_frame_rate).toBe('10/1')
  expect(audio?.codec_name).toBe('aac')
  expect(Number(probe.format?.duration)).toBeGreaterThanOrEqual(0.9)
  expect(Number(probe.format?.duration)).toBeLessThanOrEqual(1.2)

  expect(apiRequests).toContain('POST /api/upload')
  expect(apiRequests).toContain('POST /api/compositions/render')
  expect(apiRequests.some((entry) => entry === `GET /api/jobs/${jobId}`)).toBe(true)
})
