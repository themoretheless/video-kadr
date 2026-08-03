import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  enqueue: vi.fn(),
  state: {
    edit: { format: 'mp4', codec: 'h264', qualityTier: 'high' }, capabilities: { formats: [], codecs: [] },
    exporting: false,
  },
}))

vi.mock('../../store', () => ({
  state: mocks.state,
  currentBrowserExportPlan: () => null,
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
      { name: 'Web', format: 'mp4', codec: 'h264', qualityTier: 'high' },
      { name: 'Archive', format: 'mp4', codec: 'h264', qualityTier: 'high' },
    ])
    expect(host.textContent).toContain('неизменяемый снимок')
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
