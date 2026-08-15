// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.svelte'
import {
  addMediaInfoToComposition,
  compositionState,
  editorMode,
  resetCompositionForTests,
  setCompositionPlayhead,
} from '$lib/state/composition.svelte.js'
import {
  resetShortcutBindings,
  setShortcutBinding,
  setShortcutStorageForTests,
  shortcutState,
} from '$lib/state/shortcuts.svelte.js'
import { activateTimeline, defaultEdit, state as legacyState } from '$lib/state/store.svelte.js'

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>()
  get length(): number { return this.values.size }
  clear(): void { this.values.clear() }
  getItem(key: string): string | null { return this.values.get(key) ?? null }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null }
  removeItem(key: string): void { this.values.delete(key) }
  setItem(key: string, value: string): void { this.values.set(key, value) }
}

let target: HTMLDivElement

beforeEach(() => {
  setShortcutStorageForTests(new MemoryStorage())
  resetShortcutBindings()
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
    const path = String(input)
    if (path === '/api/capabilities') {
      return new Response(JSON.stringify({
        schemaVersion: 1,
        toolFingerprint: 'test',
        formats: [],
        codecs: [],
        filters: [],
        hardware: [],
        features: [{ id: 'composition-v1', label: 'Composition v1', available: true }],
      }), { status: 200 })
    }
    return new Response('[]', { status: 200 })
  }))
  resetCompositionForTests()
  compositionState.ui.snapEnabled = true
  editorMode.value = 'composition'
  legacyState.library = []
  legacyState.video = null
  legacyState.timelineSelectedSegmentId = null
  addMediaInfoToComposition({
    id: 'shortcut-video',
    url: '/files/sources/shortcut.mp4',
    filename: 'shortcut.mp4',
    mediaType: 'video',
    duration: 5,
    width: 1280,
    height: 720,
    fps: 25,
    acodec: 'aac',
  })
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  setShortcutStorageForTests(undefined)
  vi.unstubAllGlobals()
  target?.remove()
})

function key(targetNode: EventTarget, value: string, options: KeyboardEventInit = {}): void {
  targetNode.dispatchEvent(new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true, ...options }))
}

describe('composition keyboard shortcuts', () => {
  it('supports frame step, split, delete, undo/redo and snapping while ignoring form controls', async () => {
    const component = mount(App, { target })
    await tick()

    expect(compositionState.ui.snapEnabled).toBe(true)
    key(window, 'n')
    expect(compositionState.ui.snapEnabled).toBe(false)

    key(window, '.')
    expect(compositionState.transport.playheadTicks).toBe(40_000)

    setCompositionPlayhead(2_000_000)
    key(window, 'b')
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(2)

    key(window, 'd', { metaKey: true })
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(3)
    key(window, 'Delete')
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(2)
    key(window, 'z', { metaKey: true })
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(3)
    key(window, 'z', { metaKey: true, shiftKey: true })
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(2)

    await vi.waitFor(() => {
      expect(target.querySelector('[aria-label="Название композиции"]')).not.toBeNull()
    })
    const projectName = target.querySelector<HTMLInputElement>('[aria-label="Название композиции"]')!
    projectName.focus()
    key(projectName, 'n')
    expect(compositionState.ui.snapEnabled).toBe(false)

    const textarea = document.createElement('textarea')
    const select = document.createElement('select')
    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', 'true')
    target.append(textarea, select, editable)
    for (const interactive of [textarea, select, editable]) {
      interactive.focus()
      key(interactive, 'n')
      expect(compositionState.ui.snapEnabled).toBe(false)
    }

    const splitButton = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Разрезать'))!
    splitButton.focus()
    key(splitButton, ' ')
    expect(compositionState.transport.playing).toBe(false)

    await unmount(component)
  })

  it('dispatches a customized mapping and keeps frame step equal to one canvas frame', async () => {
    const component = mount(App, { target })
    await tick()
    expect(setShortcutBinding('composition.toggleSnapping', 'KeyS').ok).toBe(true)

    key(window, 'n')
    expect(compositionState.ui.snapEnabled).toBe(true)
    key(window, 's')
    expect(compositionState.ui.snapEnabled).toBe(false)

    setCompositionPlayhead(0)
    key(window, '.')
    expect(compositionState.transport.playheadTicks).toBe(
      Math.round(1_000_000 / compositionState.document.canvas.fps),
    )

    await unmount(component)
  })

  it('offers an accessible capture dialog, detects conflicts and resets defaults', async () => {
    const component = mount(App, { target })
    await tick()
    const open = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Клавиши'))!
    expect(open.getAttribute('aria-haspopup')).toBe('dialog')
    open.click()
    await tick()

    const dialog = target.querySelector<HTMLDialogElement>('dialog[aria-modal="true"]')!
    expect(dialog.open).toBe(true)
    expect(dialog.querySelector('h2')?.textContent).toContain('Сочетания клавиш')

    const snappingRow = [...dialog.querySelectorAll('tr')].find((row) => row.textContent?.includes('Переключить магнит'))!
    const snappingCapture = snappingRow.querySelector<HTMLButtonElement>('.shortcut-capture')!
    snappingCapture.click()
    key(snappingCapture, 's')
    await tick()
    expect(shortcutState.bindings['composition.toggleSnapping']).toBe('KeyS')

    const splitRow = [...dialog.querySelectorAll('tr')].find((row) => row.textContent?.includes('Разрезать клип'))!
    const splitCapture = splitRow.querySelector<HTMLButtonElement>('.shortcut-capture')!
    splitCapture.click()
    key(splitCapture, 's')
    await tick()
    expect(dialog.querySelector('[role="alert"]')?.textContent).toContain('Переключить магнит')
    expect(shortcutState.bindings['composition.split']).toBe('KeyB')

    const reset = [...dialog.querySelectorAll('button')].find((button) => button.textContent?.includes('Сбросить по умолчанию'))!
    reset.click()
    expect(shortcutState.bindings['composition.toggleSnapping']).toBe('KeyN')

    await unmount(component)
  })

  it('dispatches primary commands for the selected legacy timeline segment', async () => {
    const component = mount(App, { target })
    await tick()
    legacyState.video = {
      id: 'legacy-shortcut-video',
      url: '/files/sources/legacy.mp4',
      filename: 'legacy.mp4',
      duration: 10,
      width: 1280,
      height: 720,
      fps: 25,
    }
    legacyState.edit = defaultEdit()
    legacyState.edit.trimEnd = 10
    legacyState.playerTime = 4
    legacyState.timelineSelectedSegmentId = activateTimeline()

    // Dispatch synchronously before Svelte swaps the composition DOM for the
    // legacy panel; this test exercises App's global command bridge directly.
    editorMode.value = 'legacy'
    key(window, 'b')
    expect(legacyState.edit.timelineSegments).toHaveLength(2)

    key(window, 'd', { metaKey: true })
    expect(legacyState.edit.timelineSegments).toHaveLength(3)
    const duplicatedId = legacyState.timelineSelectedSegmentId
    key(window, 'ArrowLeft', { altKey: true })
    expect(legacyState.edit.timelineSegments[1]?.id).toBe(duplicatedId)

    key(window, 'Delete')
    expect(legacyState.edit.timelineSegments).toHaveLength(2)

    editorMode.value = 'composition'
    await tick()
    await unmount(component)
  })
})
