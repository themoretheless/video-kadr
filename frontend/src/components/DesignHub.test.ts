import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  load: vi.fn(), saveTemplate: vi.fn(), instantiate: vi.fn(), createKit: vi.fn(), deleteTemplate: vi.fn(), deleteKit: vi.fn(),
  state: { templates: [] as Array<Record<string, unknown>>, kits: [] as Array<Record<string, unknown>>, busy: false, error: '', notice: '' },
}))
vi.mock('../design-store', () => ({
  designState: mocks.state, loadDesignCatalog: mocks.load, saveCurrentProjectTemplate: mocks.saveTemplate,
  instantiateTemplate: mocks.instantiate, createBrandKit: mocks.createKit,
  deleteTemplate: mocks.deleteTemplate, deleteBrandKit: mocks.deleteKit,
}))
import DesignHub from './DesignHub.vue'

describe('DesignHub', () => {
  let host: HTMLDivElement
  let app: ReturnType<typeof createApp> | null
  beforeEach(() => { host = document.createElement('div'); document.body.append(host); app = null; Object.assign(mocks.state, { templates: [], kits: [], busy: false, error: '', notice: '' }); vi.clearAllMocks() })
  afterEach(() => { app?.unmount(); host.remove() })
  async function mount(hasActiveProject = true) { app = createApp(DesignHub, { hasActiveProject }); app.mount(host); await nextTick() }
  const button = (name: string) => [...host.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.includes(name))!

  it('loads only when mounted and exposes keyboard-native pressed section buttons', async () => {
    expect(mocks.load).not.toHaveBeenCalled(); await mount()
    expect(mocks.load).toHaveBeenCalledOnce()
    expect(host.querySelectorAll('[role="tab"]')).toHaveLength(0)
    const sections = host.querySelectorAll<HTMLButtonElement>('[role="group"] > button'); expect(sections).toHaveLength(2); expect(sections[0]?.getAttribute('aria-pressed')).toBe('true')
    button('Brand kits').click(); await nextTick(); expect(sections[1]?.getAttribute('aria-pressed')).toBe('true')
  })

  it('creates a typed placeholder snapshot and rejects duplicate labels', async () => {
    await mount(); button('Добавить placeholder').click(); button('Добавить placeholder').click(); await nextTick()
    const labels = host.querySelectorAll<HTMLInputElement>('input[aria-label^="Название placeholder"]')
    for (const input of labels) { input.value = 'Hero'; input.dispatchEvent(new Event('input', { bubbles: true })) }
    const name = host.querySelector<HTMLInputElement>('input[required]')!; name.value = 'Launch'; name.dispatchEvent(new Event('input', { bubbles: true })); await nextTick()
    button('Сохранить шаблон').click(); await nextTick(); expect(mocks.saveTemplate).not.toHaveBeenCalled(); expect(host.querySelector('[role="alert"]')?.textContent).toContain('должны отличаться')
    labels[1]!.value = 'Title'; labels[1]!.dispatchEvent(new Event('input', { bubbles: true })); button('Сохранить шаблон').click(); await nextTick()
    expect(mocks.saveTemplate).toHaveBeenCalledWith('Launch', [
      { label: 'Hero', kind: 'video', required: true }, { label: 'Title', kind: 'video', required: true },
    ])
  })

  it('creates a normalized brand kit and confirms destructive deletion', async () => {
    mocks.state.kits = [{ id: 'kit-1', name: 'Acme', revision: 2, colors: [], fonts: [], logos: [] }]
    await mount(); button('Brand kits').click(); await nextTick()
    const textInputs = host.querySelectorAll<HTMLInputElement>('input[type="text"]')
    const name = host.querySelector<HTMLInputElement>('[aria-label="Название Brand kit"]')!
    name.value = 'Studio'; name.dispatchEvent(new Event('input', { bubbles: true })); await nextTick()
    const colorName = host.querySelector<HTMLInputElement>('[aria-label="Название цвета 1"]')!; colorName.value = 'Accent'; colorName.dispatchEvent(new Event('input', { bubbles: true }))
    const hex = host.querySelector<HTMLInputElement>('[aria-label="HEX цвета 1"]')!; hex.value = '#AABBCC'; hex.dispatchEvent(new Event('input', { bubbles: true }))
    button('Сохранить Brand kit').click(); await nextTick(); expect(mocks.createKit).toHaveBeenCalledWith({ name: 'Studio', colors: [{ name: 'Accent', value: '#aabbcc' }], fontFiles: [], logoFiles: [] })
    expect(textInputs.length).toBeGreaterThan(0)
    const invoker = button('Удалить'); invoker.focus(); invoker.click(); await nextTick(); expect(host.querySelector('[role="alertdialog"]')?.textContent).toContain('Acme'); expect(mocks.deleteKit).not.toHaveBeenCalled()
    expect(document.activeElement?.textContent).toContain('Отмена')
    host.querySelector<HTMLElement>('[role="alertdialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await nextTick(); expect(host.querySelector('[role="alertdialog"]')).toBeNull(); expect(document.activeElement).toBe(invoker)
    invoker.click(); await nextTick()
    const deleteButtons = [...host.querySelectorAll<HTMLButtonElement>('button')].filter(item => item.textContent?.trim() === 'Удалить'); deleteButtons.at(-1)!.click(); expect(mocks.deleteKit).toHaveBeenCalledWith('kit-1')
  })

  it('confirms project replacement before applying a template and restores focus on cancel', async () => {
    mocks.state.templates = [{ id: 'template-1', name: 'Launch', description: '', revision: 3, placeholders: [] }]
    await mount(true)
    const apply = button('Применить'); apply.focus(); apply.click(); await nextTick()
    const dialog = host.querySelector<HTMLElement>('[role="alertdialog"]')!
    expect(dialog.textContent).toContain('Текущий проект будет закрыт и заменён')
    expect(mocks.instantiate).not.toHaveBeenCalled()
    expect(document.activeElement?.textContent).toContain('Отмена')
    button('Отмена').click(); await nextTick(); expect(document.activeElement).toBe(apply)
    apply.click(); await nextTick(); button('Создать новый проект').click(); await nextTick()
    expect(mocks.instantiate).toHaveBeenCalledWith('template-1')
  })

  it('disables template creation without an active project', async () => {
    await mount(false); expect(host.textContent).toContain('Откройте проект'); expect(host.querySelector('fieldset')?.disabled).toBe(true)
  })
})
