import { chromium, expect } from '@playwright/test'
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

  async function download() {
    const pending = page.waitForEvent('download')
    await page.getByRole('button', { name: 'Скачать проект', exact: true }).click()
    return readFile(await (await pending).path())
  }

  const operations = [
    ['Вставить исходник', 'Исходник вставлен'],
    ['Записать исходник поверх', 'Исходник записан поверх'],
  ]
  for (const [label, status] of operations) {
    await page.goto(new URL('e2e/fixtures/live-filmcraft-program.html', url).href,{waitUntil:'domcontentloaded'})
    await page.getByText('Проект Filmcraft',{exact:true}).click()
    await open(Buffer.from(JSON.stringify(original)))
    await page.locator('summary').filter({hasText:/^Точки In\/Out$/}).click()
    await page.getByLabel('Монитор точек Filmcraft',{exact:true}).selectOption('source')
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
    await page.getByRole('button',{name:'Отменить правку',exact:true}).click()
    assert.deepEqual(JSON.parse(await download()),JSON.parse(before))
    await page.getByRole('button',{name:'Повторить правку',exact:true}).click()
    assert.deepEqual(JSON.parse(await download()),JSON.parse(changed))
    await open(changed)
    assert.deepEqual(JSON.parse(await download()),JSON.parse(changed))
    console.log('PASS',label,'undo redo download reopen')
  }
} finally {
  await browser.close()
}
