import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { expect, test, type Page } from '@playwright/test'

const require = createRequire(import.meta.url)
const axeSource = readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8')

async function mockBackend(page: Page, offline: boolean): Promise<void> {
  await page.route('**/api/**', async (route) => {
    if (offline) {
      await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ error: 'backend unavailable' }) })
      return
    }
    const path = new URL(route.request().url()).pathname
    const body = path === '/api/capabilities'
      ? { schemaVersion: 1, toolFingerprint: 'a11y', formats: [], codecs: [], filters: [], hardware: [] }
      : []
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) })
  })
}

async function assertNoSeriousAxeViolations(page: Page): Promise<void> {
  await page.addScriptTag({ content: axeSource })
  const violations = await page.evaluate(async () => {
    const runtime = window as typeof window & {
      axe: { run: (root: Document, options: unknown) => Promise<{ violations: Array<{ id: string; impact: string | null; nodes: Array<{ target: unknown; html: string }> }> }> }
    }
    const result = await runtime.axe.run(document, {
      runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] },
    })
    return result.violations
      .filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
      .map((violation) => ({ id: violation.id, impact: violation.impact, nodes: violation.nodes.map((node) => ({ target: node.target, html: node.html })) }))
  })
  expect(violations).toEqual([])
}

test('stateful error, dialog and command menu pass the strict axe gate', async ({ browser }) => {
  const errorPage = await browser.newPage()
  await mockBackend(errorPage, true)
  await errorPage.goto('/')
  await expect(errorPage.getByRole('status')).toHaveText('Сервер недоступен')
  await assertNoSeriousAxeViolations(errorPage)

  await errorPage.getByRole('button', { name: /Клавиши/ }).click()
  await expect(errorPage.getByRole('dialog', { name: 'Сочетания клавиш' })).toBeVisible()
  await assertNoSeriousAxeViolations(errorPage)
  await errorPage.keyboard.press('Escape')
  await expect(errorPage.getByRole('dialog', { name: 'Сочетания клавиш' })).toBeHidden()
  await errorPage.close()

  const menuPage = await browser.newPage()
  await mockBackend(menuPage, false)
  await menuPage.goto('/')
  await menuPage.getByRole('button', { name: 'Multitrack' }).click()
  const menuButton = menuPage.getByRole('button', { name: 'Команды' })
  await menuButton.focus()
  await menuButton.press('Enter')
  await expect(menuPage.getByRole('menu', { name: 'Команды монтажной линии' })).toBeVisible()
  await assertNoSeriousAxeViolations(menuPage)
  await menuPage.getByRole('menu', { name: 'Команды монтажной линии' }).press('Escape')
  await expect(menuPage.getByRole('menu', { name: 'Команды монтажной линии' })).toBeHidden()
  await menuPage.close()
})
