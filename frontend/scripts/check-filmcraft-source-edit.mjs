import { chromium, expect } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import assert from 'node:assert/strict'

const url = new URL(process.env.FILMCRAFT_PREVIEW_URL ?? 'http://127.0.0.1:5198')
url.pathname = url.pathname.replace(/\/?$/, '/')
assert.equal((await fetch(new URL('api/health', url))).status, 200)
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE
const browser = await chromium.launch({ ...(executablePath ? { executablePath } : {}) })
try {
  const page = await browser.newPage({ acceptDownloads: true })
  const registration = await page.request.post(new URL('api/auth/register', url).href, {
    data: { username: `filmcraft-source-check-${Date.now()}`, password: 'temporary-only-test-password' },
  })
  assert.equal(registration.status(), 201)
  const { token } = await registration.json()
  await page.addInitScript(value => {
    localStorage.setItem('video-kadr.filmcraft-program-test', JSON.stringify({ token: value, library: [] }))
  }, token)
  const fixture = new URL('../../backend/tests/fixtures/filmcraft-source-edit-oracle.json', import.meta.url)
  const { profiles } = JSON.parse(await readFile(fixture, 'utf8'))
  const original = { format: 'filmcraft.project', schema_version: 12, project: profiles[0] }

  async function open(buffer) {
    await page.getByLabel('Открыть проект Filmcraft', { exact: true }).setInputFiles({
      name: 'edit.filmcraft.json', mimeType: 'application/json', buffer,
    })
    await page.getByLabel('Позиция правки, кадров', { exact: true }).waitFor()
  }

  async function download(label = 'Скачать проект') {
    const pending = page.waitForEvent('download')
    await page.getByRole('button', { name: label, exact: true }).click()
    return readFile(await (await pending).path())
  }

  async function exportFrame(index = 0) {
    await page.getByLabel('Позиция правки, кадров', { exact: true }).fill(String(index))
    const exported = page.waitForResponse(response => response.url().endsWith('/api/compositions/filmcraft/export-frame'))
      .then(async response => { if (!response.ok()) throw new Error(await response.text()) })
    const [frame] = await Promise.all([download('Экспортировать кадр PNG'), exported])
    assert.equal(frame.subarray(0, 8).toString('hex'), '89504e470d0a1a0a')
    return frame
  }

  function decodeFrames(input) {
    return execFileSync('ffmpeg', [
      '-v', 'error', '-i', 'pipe:0', '-map', '0:v:0', '-pix_fmt', 'rgba', '-f', 'rawvideo', 'pipe:1',
    ], { input, maxBuffer: 32 * 1024 * 1024, timeout: 30_000 })
  }

  async function exportVideo(startFrame) {
    await page.getByLabel('Диапазон обычного экспорта Filmcraft', { exact: true }).selectOption('frames')
    await page.getByLabel('Начальный кадр видеоэкспорта Filmcraft', { exact: true }).fill(String(startFrame))
    await page.getByLabel('Количество кадров видеоэкспорта Filmcraft', { exact: true }).fill('2')
    await page.getByLabel('Добавить звук в видеоэкспорт Filmcraft', { exact: true }).check()
    await page.getByRole('button', { name: 'Экспортировать видео со звуком', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Скачать видео MKV', exact: true })).toBeVisible({ timeout: 60_000 })
    return download('Скачать видео MKV')
  }

  const operations = [
    ['Вставить исходник', 'Исходник вставлен'],
    ['Записать исходник поверх', 'Исходник записан поверх'],
  ]
  for (const [label, status] of operations) {
    await page.goto(new URL('e2e/fixtures/live-filmcraft-program.html', url).href,{waitUntil:'domcontentloaded'})
    await page.getByText('Проект Filmcraft',{exact:true}).click()
    await open(Buffer.from(JSON.stringify(original)))
    await page.getByLabel('Секвенция Filmcraft', { exact: true }).selectOption('27')
    await page.locator('summary').filter({hasText:/^Точки In\/Out$/}).click()
    await page.getByLabel('Монитор точек Filmcraft',{exact:true}).selectOption('source')
    await page.getByLabel('Исходник точек Filmcraft', { exact: true }).selectOption('14')
    await page.getByRole('button', { name: 'Вставить исходник', exact: true }).click()
    await expect(page.getByText('Исходник вставлен', { exact: true })).toBeVisible()
    const beforeFrame = await exportFrame()
    await page.getByLabel('Исходник точек Filmcraft',{exact:true}).selectOption('12')
    for (const [time,mark] of [['0','In'],['21168000000','Out']]) {
      await page.getByLabel('Позиция точки Filmcraft',{exact:true}).fill(time)
      await page.getByRole('button',{name:'Поставить '+mark,exact:true}).click()
      await expect(page.getByText('Точки In/Out обновлены',{exact:true})).toBeVisible()
    }
    const before=await download()
    await page.getByRole('button',{name:label,exact:true}).click()
    await expect(page.getByText(status,{exact:true})).toBeVisible({timeout:15000})
    const changed=await download()
    assert(!changed.equals(before),label)
    const afterFrame = await exportFrame()
    assert(!afterFrame.equals(beforeFrame), label)
    await page.getByRole('button',{name:'Отменить правку',exact:true}).click()
    assert.deepEqual(JSON.parse(await download()),JSON.parse(before))
    await page.getByRole('button',{name:'Повторить правку',exact:true}).click()
    assert.deepEqual(JSON.parse(await download()),JSON.parse(changed))
    await open(changed)
    await page.getByLabel('Секвенция Filmcraft', { exact: true }).selectOption('27')
    assert.deepEqual(JSON.parse(await download()),JSON.parse(changed))
    assert((await exportFrame()).equals(afterFrame), label)
    const sourceSequence = JSON.parse(changed).project.items['27'].kind.Sequence
    const bars = sourceSequence.video_tracks.flatMap(track => track.items).find(clip => clip.item === 12)
    assert(bars)
    const frameTicks = 254016000000 * sourceSequence.settings.frame_rate.den / sourceSequence.settings.frame_rate.num
    const boundary = (bars.start + bars.duration) / frameTicks - 1
    assert(Number.isInteger(boundary) && boundary >= 1)
    const preTrimFrame = await exportFrame(boundary)
    await page.getByLabel('Клип Filmcraft', { exact: true }).selectOption(String(bars.id))
    await page.getByLabel('Край обрезки Filmcraft', { exact: true }).selectOption('out')
    await page.getByLabel('Сдвиг обрезки', { exact: true }).fill('-1')
    await page.getByRole('button', { name: 'Обрезать', exact: true }).click()
    const trimmed = await download()
    const trimmedSequence = JSON.parse(trimmed).project.items['27'].kind.Sequence
    const trimmedBars = trimmedSequence.video_tracks.flatMap(track => track.items).find(clip => clip.id === bars.id)
    assert.equal(trimmedBars.duration, bars.duration - frameTicks)
    assert((await exportFrame()).equals(afterFrame), label)
    const postTrimFrame = await exportFrame(boundary)
    assert(!postTrimFrame.equals(preTrimFrame), label)
    await page.getByRole('button', { name: 'Отменить правку', exact: true }).click()
    assert.deepEqual(JSON.parse(await download()), JSON.parse(changed))
    await page.getByRole('button', { name: 'Повторить правку', exact: true }).click()
    assert.deepEqual(JSON.parse(await download()), JSON.parse(trimmed))
    await open(trimmed)
    await page.getByLabel('Секвенция Filmcraft', { exact: true }).selectOption('27')
    assert((await exportFrame(boundary)).equals(postTrimFrame), label)
    const expectedFrames = Buffer.concat([decodeFrames(await exportFrame(boundary - 1)), decodeFrames(postTrimFrame)])
    const video = await exportVideo(boundary - 1)
    const { streams } = JSON.parse(execFileSync('ffprobe', [
      '-v', 'error', '-show_entries', 'stream=codec_type,codec_name,channels,sample_rate',
      '-of', 'json', 'pipe:0',
    ], { input: video, encoding: 'utf8', timeout: 30_000 }))
    assert.equal(streams.find(stream => stream.codec_type === 'video')?.codec_name, 'ffv1')
    const audioStream = streams.find(stream => stream.codec_type === 'audio')
    assert.equal(audioStream?.sample_rate, '48000')
    assert.equal(audioStream?.channels, 2)
    assert(decodeFrames(video).equals(expectedFrames), `${label}: decoded video frames differ from PNGs`)
    const audio = execFileSync('ffmpeg', [
      '-v', 'error', '-i', 'pipe:0', '-map', '0:a:0', '-f', 'f32le', 'pipe:1',
    ], { input: video, timeout: 30_000 })
    const samplesPerFrame = frameTicks * 48000 / 254016000000
    const frameAudioBytes = samplesPerFrame * 2 * Float32Array.BYTES_PER_ELEMENT
    assert.equal(audio.length, 2 * frameAudioBytes)
    assert(audio.subarray(0, frameAudioBytes).some(byte => byte !== 0), `${label}: missing Bars and Tone audio`)
    assert(audio.subarray(frameAudioBytes).every(byte => byte === 0), `${label}: sound continued beyond trim`)
    const unsupported = JSON.parse(changed)
    const sequence = unsupported.project.items['27'].kind.Sequence
    for (const track of sequence.video_tracks) {
      for (const clip of track.items) {
        const remap = clip.effects.find(effect => effect.effect === 'time_remap')
        if (remap) remap.params.speed.value.Float = 50
      }
    }
    const refused = await page.request.post(new URL('api/compositions/filmcraft/export-frame', url).href, {
      headers: { Authorization: `Bearer ${token}` },
      data: { document: unsupported, sequenceId: '27', timeFrames: '0', sources: {} },
    })
    assert.equal(refused.status(), 400)
    assert.match((await refused.json()).error, /time remapping supports only constant 100%/)
    console.log('PASS', label, 'trim, undo redo download reopen, PNG and FFV1 with sample-aligned audio')
  }
} finally {
  await browser.close()
}
