import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  enqueue: vi.fn(),
  state: {
    edit: { format: 'mp4', codec: 'h264', qualityTier: 'high' }, capabilities: {
      formats: [] as Array<{ id: string; available: boolean; reason?: string }>,
      codecs: [] as Array<{ id: string; available: boolean; reason?: string }>,
    },
    exporting: false,
    video: { duration: 60, width: 1280, height: 720, fps: 30 },
  },
}))

vi.mock('../../store', () => ({
  state: mocks.state,
  clientOnlyMode: true,
  currentBrowserExportPlan: () => null,
  buildExportSizingSnapshot: (overrides: Record<string, unknown>) => ({ payload: overrides, width: 1280, height: 720, fps: 30, selectedSeconds: 60, durationSeconds: 60, hasAudio: true, browser: true }),
  selectedExportUnavailableReason: () => null,
  hasMeaningfulChanges: () => true,
  doExport: vi.fn(), doStreamingExport: vi.fn(), streamingOutputSupported: () => false,
  enqueueExportVariants: mocks.enqueue,
  exportQueueState: { maxVariants: 8 },
}))
vi.mock('../ExportQueuePanel.vue', () => ({ default: { template: '<div />' } }))

import ExportControls from './ExportControls.vue'

describe('ExportControls batch variants', () => {
  let host: HTMLDivElement
  let app: ReturnType<typeof createApp> | null
  beforeEach(() => { host = document.createElement('div'); document.body.append(host); app = null; vi.clearAllMocks() })
  afterEach(() => { app?.unmount(); host.remove() })

  it('creates a bounded named batch and enqueues immutable variant values', async () => {
    app = createApp(ExportControls); app.mount(host); await nextTick()
    const toggle = [...host.querySelectorAll('button')].find(button => button.textContent?.includes('Пакетный экспорт'))!
    toggle.click(); await nextTick()
    const names = host.querySelectorAll<HTMLInputElement>('input[aria-label^="Название варианта"]')
    expect(names).toHaveLength(2)
    names[0]!.value = 'Web'; names[0]!.dispatchEvent(new Event('input', { bubbles: true }))
    names[1]!.value = 'Archive'; names[1]!.dispatchEvent(new Event('input', { bubbles: true }))
    const enqueue = [...host.querySelectorAll('button')].find(button => button.textContent?.includes('Поставить пакет'))!
    enqueue.click(); await nextTick()
    expect(mocks.enqueue).toHaveBeenCalledWith([
      { name: 'Web', format: 'mp4', codec: 'h264', qualityTier: 'high', rateControl: { mode: 'quality', crf: 18 } },
      { name: 'Archive', format: 'mp4', codec: 'h264', qualityTier: 'high', rateControl: { mode: 'quality', crf: 18 } },
    ])
    expect(host.textContent).toContain('неизменяемый снимок')
  })

  it('exposes an accessible target-size estimate and snapshots its rate control', async () => {
    app = createApp(ExportControls); app.mount(host); await nextTick()
    const targetMode = host.querySelector<HTMLInputElement>('input[type="radio"][value="target_size"]')!
    targetMode.click(); await nextTick()
    const target = host.querySelector<HTMLInputElement>('#target-size-mb')!
    expect(target.getAttribute('aria-describedby')).toContain('target-size-help')
    expect(host.querySelector('[role="status"]')?.textContent).toContain('Оценка размера')
    expect(host.textContent).toContain('Видеобитрейт')
    ;[...host.querySelectorAll('button')].find(button => button.textContent?.includes('Пакетный экспорт'))!.click(); await nextTick()
    const names = host.querySelectorAll<HTMLInputElement>('input[aria-label^="Название варианта"]')
    const variantTarget = host.querySelector<HTMLInputElement>('input[aria-label="Целевой размер варианта 1, МБ"]')!
    variantTarget.value = '0'; variantTarget.dispatchEvent(new Event('input', { bubbles: true })); await nextTick()
    expect(variantTarget.getAttribute('aria-invalid')).toBe('true')
    const errorId = variantTarget.getAttribute('aria-describedby')!
    expect(errorId).toMatch(/^variant-target-error-/)
    expect(document.getElementById(errorId)?.getAttribute('role')).toBe('alert')
    variantTarget.value = '25'; variantTarget.dispatchEvent(new Event('input', { bubbles: true })); await nextTick()
    names[0]!.value = 'Target'; names[0]!.dispatchEvent(new Event('input', { bubbles: true }))
    names[1]!.value = 'Quality'; names[1]!.dispatchEvent(new Event('input', { bubbles: true }))
    const enqueue = [...host.querySelectorAll('button')].find(button => button.textContent?.includes('Поставить пакет'))!
    enqueue.click(); await nextTick()
    expect(mocks.enqueue.mock.calls.at(-1)?.[0][0].rateControl).toMatchObject({ mode: 'target_size', targetBytes: 25_000_000, estimatorVersion: 'size-v1' })
  })

  it('uses native disabled semantics for unavailable format and codec capabilities', async () => {
    mocks.state.capabilities = { formats: [{ id: 'av1', available: false, reason: 'Нет AV1' }], codecs: [{ id: 'h265', available: false, reason: 'Нет H.265' }] }
    app = createApp(ExportControls); app.mount(host); await nextTick()
    expect([...host.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.trim() === 'AV1')?.disabled).toBe(true)
    expect([...host.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.trim() === 'H.265')?.disabled).toBe(true)
    mocks.state.capabilities = { formats: [], codecs: [] }
  })

  it('rejects duplicate names before enqueue', async () => {
    app = createApp(ExportControls); app.mount(host); await nextTick()
    ;[...host.querySelectorAll('button')].find(button => button.textContent?.includes('Пакетный экспорт'))!.click(); await nextTick()
    const names = host.querySelectorAll<HTMLInputElement>('input[aria-label^="Название варианта"]')
    for (const input of names) { input.value = 'Same'; input.dispatchEvent(new Event('input', { bubbles: true })) }
    ;[...host.querySelectorAll('button')].find(button => button.textContent?.includes('Поставить пакет'))!.click(); await nextTick()
    expect(mocks.enqueue).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('должны отличаться')
  })
})
