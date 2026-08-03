import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  state: { restoring: false, message: '2 варианта', tasks: [] as Array<Record<string, unknown>> },
  cancel: vi.fn(), retry: vi.fn(),
}))

vi.mock('../store', () => ({
  exportQueueState: mocks.state,
  cancelQueuedExport: mocks.cancel,
  retryQueuedExport: mocks.retry,
}))

import ExportQueuePanel from './ExportQueuePanel.vue'

let host: HTMLDivElement
let app: ReturnType<typeof createApp> | null = null

async function mount() {
  app = createApp(ExportQueuePanel); app.mount(host); await nextTick()
}

describe('ExportQueuePanel', () => {
  beforeEach(() => {
    host = document.createElement('div'); document.body.append(host)
    mocks.state.restoring = false; mocks.state.message = '2 варианта'; mocks.state.tasks = []
    vi.clearAllMocks()
  })
  afterEach(() => { app?.unmount(); app = null; host.remove() })

  it('explains honest restart semantics and exposes named accessible actions', async () => {
    mocks.state.tasks = [
      { id: 'run', name: 'Web', status: 'running', progress: 42, stage: 'Кодирование', attempt: 2 },
      { id: 'old', name: 'Archive', status: 'interrupted', error: 'Страница была перезагружена', attempt: 1 },
    ]
    await mount()
    expect(host.textContent).toContain('Определения и история очереди сохраняются')
    expect(host.textContent).toContain('только в текущей сессии')
    expect(host.textContent).toContain('не имеет checkpoint')
    expect(host.textContent).toContain('запускается заново с 0%')
    expect(host.textContent).toContain('Перезапуск с 0% · попытка 2')
    const cancel = host.querySelector<HTMLButtonElement>('[aria-label="Отменить Web"]')!
    const restart = host.querySelector<HTMLButtonElement>('[aria-label="Начать заново Archive"]')!
    cancel.click(); restart.click()
    expect(mocks.cancel).toHaveBeenCalledWith('run')
    expect(mocks.retry).toHaveBeenCalledWith('old')
    expect(host.querySelector('progress')?.getAttribute('aria-label')).toContain('42%')
  })

  it('routes LUT permission recovery to the LUT library and re-upload guidance', async () => {
    mocks.state.tasks = [{ id: 'lut-permission', name: 'Cinema', status: 'permission_required', error: 'Непрозрачная ошибка B-29', recoveryKind: 'lut', attempt: 1 }]
    await mount()
    expect(host.textContent).toContain('повторно загрузите файл .cube')
    expect(host.querySelector<HTMLAnchorElement>('a[href="#lut-library"]')?.getAttribute('aria-label')).toContain('Cinema')
    expect(host.querySelector('a[href="#media-library"]')).toBeNull()
  })

  it('offers a durable completed result without retry or cancel controls', async () => {
    mocks.state.tasks = [{ id: 'done', name: 'Mobile', status: 'done', result: { url: 'blob:result', filename: 'mobile.mp4' } }]
    await mount()
    const link = host.querySelector<HTMLAnchorElement>('a[download="mobile.mp4"]')
    expect(link?.textContent).toContain('Mobile')
    expect(link?.getAttribute('aria-label')).toContain('текущей сессии')
    expect(host.querySelector('button')).toBeNull()
  })

  it('makes permission loss recoverable or cancellable from the global queue', async () => {
    mocks.state.tasks = [{ id: 'permission', name: 'Archive', status: 'permission_required', error: 'LUT упомянут только в локализованном тексте', recoveryKind: 'source' }]
    await mount()
    expect(host.textContent).toContain('Перепривяжите исходник в медиатеке')
    expect(host.querySelector<HTMLAnchorElement>('a[href="#media-library"]')?.getAttribute('aria-label')).toContain('Archive')
    const retry = host.querySelector<HTMLButtonElement>('[aria-label="Повторить после перепривязки Archive"]')!
    const cancel = host.querySelector<HTMLButtonElement>('[aria-label="Отменить Archive"]')!
    retry.click(); cancel.click()
    expect(mocks.retry).toHaveBeenCalledWith('permission')
    expect(mocks.cancel).toHaveBeenCalledWith('permission')
  })
})
