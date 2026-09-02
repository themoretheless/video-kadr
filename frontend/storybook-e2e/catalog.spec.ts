import { expect, test } from '@playwright/test'

const stories = ['empty', 'loading', 'error', 'long-content', 'localized', 'mobile', 'reduced-motion']

for (const story of stories) {
  test(`${story} visual contract`, async ({ page }) => {
    if (story === 'mobile') await page.setViewportSize({ width: 390, height: 844 })
    if (story === 'reduced-motion') await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto(`/iframe.html?id=wave-10-state-catalog--${story}&viewMode=story`)
    await expect(page.locator('#storybook-root')).toBeVisible()
    await expect(page).toHaveScreenshot(`${story}.png`, { animations: 'disabled', fullPage: true })
  })
}
