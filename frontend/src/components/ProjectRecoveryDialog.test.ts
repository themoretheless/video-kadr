import { createApp, nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  accept: vi.fn(), discard: vi.fn(),
  recovery: { candidate: null as null | Record<string, unknown>, busy: false, error: '' },
}))

vi.mock('../store', () => ({
  projectRecovery: mocks.recovery,
  acceptProjectRecovery: mocks.accept,
  discardAutosaveRecovery: mocks.discard,
}))

import ProjectRecoveryDialog from './ProjectRecoveryDialog.vue'

describe('ProjectRecoveryDialog', () => {
  let host: HTMLElement
  beforeEach(() => {
    mocks.accept.mockReset()
    mocks.discard.mockReset()
    mocks.recovery.busy = false
    mocks.recovery.error = ''
    mocks.recovery.candidate = null
    host = document.createElement('div')
    document.body.append(host)
  })

  function mountDialog() { createApp(ProjectRecoveryDialog).mount(host); return host }

  it('offers explicit recover and saved-version choices for a crash draft', async () => {
    mocks.recovery.candidate = {
      projectId: 'project-1', videoId: 'asset-1', reason: 'draft', journal: true,
      corruptRevision: 1, candidateRevision: 2,
      candidate: { name: 'Черновик', updatedAt: Date.now() },
    }
    const element = mountDialog()
    await nextTick()
    expect(element.querySelector('[role="alertdialog"]')?.getAttribute('aria-modal')).toBe('true')
    expect(element.textContent).toContain('Черновик')
    const buttons = element.querySelectorAll<HTMLButtonElement>('button')
    buttons[0]!.focus()
    element.querySelector<HTMLElement>('[role="alertdialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.activeElement).toBe(buttons[1])
    element.querySelector<HTMLElement>('[role="alertdialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.activeElement).toBe(buttons[0])
    element.querySelector<HTMLElement>('[role="alertdialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(element.querySelector('[role="alertdialog"]')).not.toBeNull()
    element.querySelector<HTMLButtonElement>('button.btn.primary')!.click()
    expect(mocks.accept).toHaveBeenCalledOnce()
    element.querySelector<HTMLButtonElement>('button.btn.ghost')!.click()
    expect(mocks.discard).toHaveBeenCalledOnce()
    host.remove()
  })

  it('never offers silent discard for a corrupt head', () => {
    mocks.recovery.candidate = {
      projectId: 'project-1', videoId: 'asset-1', reason: 'corrupt', journal: false,
      corruptRevision: 4, candidateRevision: 3,
      candidate: { name: 'Последняя исправная', updatedAt: Date.now() },
    }
    const element = mountDialog()
    expect(element.textContent).toContain('Последнее сохранение повреждено')
    expect(element.querySelectorAll('button')).toHaveLength(1)
    host.remove()
  })

  it('keeps recovery actionable while announcing a storage error', () => {
    mocks.recovery.error = 'IndexedDB временно недоступен'
    mocks.recovery.candidate = {
      projectId: 'project-1', videoId: 'asset-1', reason: 'draft', journal: true,
      corruptRevision: 1, candidateRevision: 2,
      candidate: { name: 'Черновик', updatedAt: Date.now() },
    }
    const element = mountDialog()
    expect(element.querySelector('[role="alert"]')?.textContent).toContain('IndexedDB')
    expect([...element.querySelectorAll<HTMLButtonElement>('button')].every((button) => !button.disabled)).toBe(true)
    host.remove()
  })

  it('blocks autosave without offering destructive actions when no valid snapshot exists', () => {
    mocks.recovery.candidate = {
      projectId: 'project-1', videoId: 'asset-1', reason: 'corrupt', journal: false,
      corruptRevision: 4, candidateRevision: null, candidate: null,
    }
    const element = mountDialog()
    expect(element.textContent).toContain('Автосохранение заблокировано')
    expect(element.querySelector('button')?.textContent).toContain('Вернуться в медиатеку')
    host.remove()
  })
})
