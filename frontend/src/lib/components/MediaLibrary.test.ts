// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { state } from '$lib/state/store.svelte.js'
import { proxyController } from '$lib/proxy/state.svelte.js'
import type { MediaEntry } from '$lib/types.js'
import MediaLibrary from './MediaLibrary.svelte'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | null = null

function libraryItems(): MediaEntry[] {
  return [
    {
      id: 'voice/one',
      kind: 'source',
      filename: 'voice.webm',
      url: '/files/sources/voice.webm',
      mediaType: 'audio',
      title: 'Client interview',
      favorite: true,
      tags: ['Client A', 'Draft'],
      duration: 12,
      createdAt: 2,
    },
    {
      id: 'video-two',
      kind: 'source',
      filename: 'b-roll.mp4',
      url: '/files/sources/b-roll.mp4',
      mediaType: 'video',
      title: 'City B-roll',
      favorite: false,
      tags: ['Exterior'],
      duration: 20,
      width: 1920,
      height: 1080,
      createdAt: 1,
    },
  ]
}

async function settle(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
  await tick()
}

function inputByLabel(label: string): HTMLInputElement {
  const result = [...target.querySelectorAll('label')].find((candidate) =>
    candidate.textContent?.includes(label),
  )?.querySelector('input')
  if (!(result instanceof HTMLInputElement)) throw new Error(`Input not found: ${label}`)
  return result
}

function buttonByText(root: ParentNode, text: string): HTMLButtonElement {
  const result = [...root.querySelectorAll('button')].find(
    (candidate) => candidate.textContent?.trim() === text,
  )
  if (!(result instanceof HTMLButtonElement)) throw new Error(`Button not found: ${text}`)
  return result
}

beforeEach(() => {
  proxyController.resetForTests()
  state.library = libraryItems()
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  vi.unstubAllGlobals()
  target.remove()
  state.library = []
  state.capabilities = null
  proxyController.resetForTests()
})

describe('MediaLibrary', () => {
  it('lazy-loads same-origin thumbnails with a fixed-frame skeleton and fallback', async () => {
    component = mount(MediaLibrary, { target })
    await settle()

    const videoRow = target.querySelector('[data-library-id="video-two"]')
    if (!(videoRow instanceof HTMLElement)) throw new Error('Video row not found')
    const frame = videoRow.querySelector('.library-thumbnail')
    const image = frame?.querySelector('img')
    expect(frame?.getAttribute('data-thumbnail-state')).toBe('loading')
    expect(frame?.querySelector('.thumbnail-skeleton')).not.toBeNull()
    expect(image?.getAttribute('src')).toBe('/api/library/video-two/thumbnail')
    expect(image?.getAttribute('loading')).toBe('lazy')
    expect(image?.getAttribute('decoding')).toBe('async')
    expect(image?.getAttribute('src')).not.toBe('/files/sources/b-roll.mp4')

    image?.dispatchEvent(new Event('load'))
    await tick()
    expect(frame?.getAttribute('data-thumbnail-state')).toBe('ready')
    expect(frame?.querySelector('.thumbnail-skeleton')).toBeNull()
    expect(frame?.querySelector('img')?.classList.contains('loaded')).toBe(true)

    frame?.querySelector('img')?.dispatchEvent(new Event('error'))
    await tick()
    expect(frame?.getAttribute('data-thumbnail-state')).toBe('error')
    expect(frame?.querySelector('img')).toBeNull()
    expect(frame?.querySelector('[role="img"]')?.getAttribute('aria-label')).toContain(
      'недоступен',
    )

    const legacyInvalidId = target.querySelector('[data-library-id="voice/one"]')
    expect(legacyInvalidId?.querySelector('img')).toBeNull()
    expect(legacyInvalidId?.querySelector('[role="img"]')).not.toBeNull()
  })

  it('loads a video filmstrip only on hover or focus and scrubs by pointer and keyboard', async () => {
    component = mount(MediaLibrary, { target })
    await settle()

    const videoRow = target.querySelector('[data-library-id="video-two"]')
    const frame = videoRow?.querySelector('.library-thumbnail')
    if (!(frame instanceof HTMLElement)) throw new Error('Video thumbnail not found')
    expect(frame.getAttribute('role')).toBe('slider')
    expect(frame.tabIndex).toBe(0)
    expect(frame.getAttribute('data-filmstrip-state')).toBe('idle')
    expect(frame.querySelector('.filmstrip-sheet')).toBeNull()
    expect(target.querySelector('[src="/api/library/video-two/filmstrip"]')).toBeNull()
    expect(target.querySelector('video, audio')).toBeNull()

    frame.dispatchEvent(new Event('pointerenter'))
    await tick()
    const sheet = frame.querySelector('.filmstrip-sheet')
    if (!(sheet instanceof HTMLImageElement)) throw new Error('Filmstrip sheet not loaded')
    expect(sheet.getAttribute('src')).toBe('/api/library/video-two/filmstrip')
    expect(sheet.getAttribute('aria-hidden')).toBe('true')
    expect(frame.getAttribute('data-filmstrip-state')).toBe('loading')
    sheet.dispatchEvent(new Event('load'))
    await tick()
    expect(frame.getAttribute('data-filmstrip-state')).toBe('ready')
    expect(frame.classList.contains('filmstrip-active')).toBe(true)
    expect(frame.querySelector('img[alt^="Предпросмотр"]')).not.toBeNull()

    vi.spyOn(frame, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 112,
      bottom: 63,
      width: 112,
      height: 63,
      toJSON: () => ({}),
    })
    frame.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: 111 }))
    await tick()
    expect(frame.getAttribute('aria-valuenow')).toBe('7')
    expect(sheet.style.transform).toBe('translateX(-87.5%)')
    expect(frame.textContent).toContain('8/8')

    frame.dispatchEvent(new Event('pointerleave'))
    await tick()
    expect(frame.classList.contains('filmstrip-active')).toBe(false)
    expect(frame.querySelector('.filmstrip-sheet')).toBe(sheet)

    frame.focus()
    await tick()
    expect(frame.classList.contains('filmstrip-active')).toBe(true)
    frame.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
    await tick()
    expect(frame.getAttribute('aria-valuenow')).toBe('0')
    expect(frame.getAttribute('aria-valuetext')).toContain('Кадр 1 из 8')
    frame.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(frame.getAttribute('aria-valuenow')).toBe('1')

    sheet.dispatchEvent(new Event('error'))
    await tick()
    const fallbackFrame = videoRow?.querySelector('.library-thumbnail')
    expect(fallbackFrame?.getAttribute('role')).toBeNull()
    expect(fallbackFrame?.getAttribute('data-filmstrip-state')).toBe('idle')
    expect(fallbackFrame?.querySelector('.filmstrip-sheet')).toBeNull()
    expect(fallbackFrame?.querySelector('img[alt^="Предпросмотр"]')).not.toBeNull()

    const audioFrame = target
      .querySelector('[data-library-id="voice/one"]')
      ?.querySelector('.library-thumbnail')
    expect(audioFrame?.getAttribute('role')).toBeNull()
    audioFrame?.dispatchEvent(new Event('pointerenter'))
    await tick()
    expect(audioFrame?.querySelector('.filmstrip-sheet')).toBeNull()
  })

  it('searches all metadata fields and combines that with the favorite filter', async () => {
    component = mount(MediaLibrary, { target })
    await settle()

    const search = inputByLabel('Поиск по файлу')
    search.value = 'client audio'
    search.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect([...target.querySelectorAll('[data-library-id]')].map((row) => row.getAttribute('data-library-id')))
      .toEqual(['voice/one'])

    search.value = ''
    search.dispatchEvent(new Event('input', { bubbles: true }))
    const favorites = inputByLabel('Только избранное')
    favorites.checked = true
    favorites.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(target.textContent).toContain('1 из 2')
    expect(target.querySelector('[data-library-id="video-two"]')).toBeNull()
  })

  it('persists favorite, title and ordered tags through accessible controls', async () => {
    const fetchMock = vi.fn(async (path: string, init?: RequestInit) => {
      const encodedId = path.split('/')[3] ?? ''
      const id = decodeURIComponent(encodedId)
      const current = state.library.find((entry) => entry.id === id)
      if (!current) return new Response('missing', { status: 404 })
      const patch = JSON.parse(String(init?.body)) as Partial<MediaEntry>
      return new Response(JSON.stringify({ ...current, ...patch }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(MediaLibrary, { target })
    await settle()

    const videoRow = target.querySelector('[data-library-id="video-two"]')
    if (!(videoRow instanceof HTMLElement)) throw new Error('Video row not found')
    const favorite = videoRow.querySelector('button[aria-pressed="false"]')
    if (!(favorite instanceof HTMLButtonElement)) throw new Error('Favorite button not found')
    favorite.click()
    await vi.waitFor(() => {
      expect(state.library.find(({ id }) => id === 'video-two')?.favorite).toBe(true)
    })
    await tick()

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/video-two/metadata',
      expect.objectContaining({ method: 'PATCH' }),
    )
    expect(JSON.parse(String((fetchMock.mock.calls[0]?.[1] as RequestInit).body))).toEqual({
      favorite: true,
    })

    buttonByText(videoRow, 'Данные').click()
    await tick()
    const form = videoRow.querySelector('form')
    if (!(form instanceof HTMLFormElement)) throw new Error('Metadata form not found')
    const title = [...form.querySelectorAll('label')]
      .find((candidate) => candidate.textContent?.includes('Название'))
      ?.querySelector('input')
    const tags = [...form.querySelectorAll('label')]
      .find((candidate) => candidate.textContent?.includes('Теги'))
      ?.querySelector('input')
    if (!(title instanceof HTMLInputElement) || !(tags instanceof HTMLInputElement)) {
      throw new Error('Metadata inputs not found')
    }
    title.value = '  Final city cut  '
    title.dispatchEvent(new Event('input', { bubbles: true }))
    tags.value = 'Approved, Exterior'
    tags.dispatchEvent(new Event('input', { bubbles: true }))
    buttonByText(form, 'Сохранить').click()
    await vi.waitFor(() => {
      expect(state.library.find(({ id }) => id === 'video-two')?.title).toBe('Final city cut')
    })
    await tick()

    expect(state.library.find(({ id }) => id === 'video-two')).toMatchObject({
      title: 'Final city cut',
      favorite: true,
      tags: ['Approved', 'Exterior'],
    })
    expect(JSON.parse(String((fetchMock.mock.calls[1]?.[1] as RequestInit).body))).toEqual({
      title: 'Final city cut',
      tags: ['Approved', 'Exterior'],
    })
    expect(videoRow.textContent).toContain('Final city cut')
    expect(videoRow.textContent).toContain('Approved')
  })

  it('opens accessible proxy controls only for a source video', async () => {
    state.capabilities = {
      schemaVersion: 1,
      toolFingerprint: 'test',
      formats: [{ id: 'mp4', label: 'MP4', available: true }],
      codecs: [{ id: 'h264', label: 'H.264', available: true }],
      filters: [],
      hardware: [],
    }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({
      sourceId: 'video-two',
      sourceFingerprint: 'a'.repeat(64),
      status: 'none',
      proxies: [],
      jobs: [],
    }), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    component = mount(MediaLibrary, { target })
    await settle()

    const videoRow = target.querySelector('[data-library-id="video-two"]')
    if (!(videoRow instanceof HTMLElement)) throw new Error('Video row not found')
    const proxyButton = buttonByText(videoRow, 'Proxy')
    expect(proxyButton.getAttribute('aria-expanded')).toBe('false')
    proxyButton.click()
    await tick()
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/api/library/video-two/proxies', undefined))
    expect(proxyButton.getAttribute('aria-expanded')).toBe('true')
    expect(videoRow.querySelector('section[aria-label^="Proxy"]')).not.toBeNull()

    const audioRow = target.querySelector('[data-library-id="voice/one"]')
    expect(audioRow?.textContent).not.toContain('Proxy')
  })
})
