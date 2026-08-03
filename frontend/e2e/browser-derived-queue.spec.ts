import { test, expect } from '@playwright/test'

function wavFixture(sampleRate = 8_000): Buffer {
  const samples = sampleRate
  const buffer = Buffer.alloc(44 + samples * 2)
  buffer.write('RIFF', 0); buffer.writeUInt32LE(buffer.length - 8, 4); buffer.write('WAVEfmt ', 8)
  buffer.writeUInt32LE(16, 16); buffer.writeUInt16LE(1, 20); buffer.writeUInt16LE(1, 22)
  buffer.writeUInt32LE(sampleRate, 24); buffer.writeUInt32LE(sampleRate * 2, 28)
  buffer.writeUInt16LE(2, 32); buffer.writeUInt16LE(16, 34); buffer.write('data', 36)
  buffer.writeUInt32LE(samples * 2, 40)
  return buffer
}

test('derived source analysis survives a hard reload without duplicate work', async ({ page }) => {
  await page.goto('/?processing=browser')
  await page.locator('.dropzone input[type=file]').setInputFiles({
    name: 'queued-analysis.wav', mimeType: 'audio/wav', buffer: wavFixture(),
  })
  const center = page.getByRole('region', { name: 'Фоновые задачи' })
  await expect(center).toBeVisible()
  await expect(center).toContainText('probe')
  await expect(center).toContainText('Готово')
  await page.reload()
  await expect(center).toBeVisible()
  await expect(center.getByText('probe')).toHaveCount(1)
  await expect(center).toContainText('возобновятся при следующем открытии')
})
