// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  addMediaInfoToComposition,
  compositionState,
  resetCompositionForTests,
  setCompositionPlayhead,
} from '$lib/state/composition.svelte.js'
import CompositionReviewPanel from './CompositionReviewPanel.svelte'
import { setAuthSessionForTests } from '$lib/state/auth.svelte.js'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined

beforeEach(() => {
  setAuthSessionForTests({
    user: { id: 'user-1', username: 'local-owner', createdAt: 1 },
    token: 'owner-session',
    expiresAt: 2_000_000_000,
  })
  resetCompositionForTests()
  addMediaInfoToComposition({
    id: 'review-video',
    url: '/files/sources/review.mp4',
    filename: 'review.mp4',
    mediaType: 'video',
    duration: 5,
    width: 1280,
    height: 720,
  })
  compositionState.projectId = 'project/one'
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  setAuthSessionForTests(null)
  vi.unstubAllGlobals()
  target.remove()
})

async function settle(): Promise<void> {
  await tick()
  await Promise.resolve()
  await tick()
}

function button(label: string): HTMLButtonElement {
  const found = [...target.querySelectorAll('button')].find((item) => item.textContent?.includes(label))
  if (!found) throw new Error(`Missing button: ${label}`)
  return found
}

describe('CompositionReviewPanel', () => {
  it('creates, seeks, replies and resolves a timeline review thread', async () => {
    let thread = {
      id: 'thread-1',
      projectId: 'project/one',
      comments: [{ id: 'comment-1', author: 'local-owner', body: 'Tighten cut', timelineTick: 1_500_000, createdAt: 1 }],
      resolvedAt: null as number | null,
      resolvedBy: null as string | null,
    }
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (method === 'GET') return new Response('[]', { status: 200 })
      if (method === 'POST' && String(_input).endsWith('/replies')) {
        const body = JSON.parse(String(init?.body)) as { body: string }
        thread = { ...thread, comments: [...thread.comments, { id: 'reply-1', author: 'local-owner', body: body.body, timelineTick: 1_500_000, createdAt: 2 }] }
      } else if (method === 'PUT') {
        thread = { ...thread, resolvedAt: 3, resolvedBy: 'local-owner' }
      }
      return new Response(JSON.stringify(thread), { status: method === 'POST' ? 201 : 200 })
    })
    vi.stubGlobal('fetch', fetchMock)
    setCompositionPlayhead(1_500_000)
    component = mount(CompositionReviewPanel, { target })
    await settle()

    const comment = target.querySelector<HTMLTextAreaElement>('#composition-review-comment')!
    comment.value = 'Tighten cut'
    comment.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    button('Добавить комментарий').click()
    await vi.waitFor(() => expect(target.querySelector('article')?.textContent).toContain('Tighten cut'))
    expect(JSON.parse(String((fetchMock.mock.calls[3]![1] as RequestInit).body))).toEqual({
      body: 'Tighten cut', timelineTick: 1_500_000,
    })
    expect((fetchMock.mock.calls[3]![1] as RequestInit).headers).toMatchObject({ Authorization: 'Bearer owner-session' })

    setCompositionPlayhead(0)
    button('0:01.500').click()
    expect(compositionState.transport.playheadTicks).toBe(1_500_000)

    button('Ответить').click()
    await settle()
    const reply = target.querySelector<HTMLTextAreaElement>('#composition-review-reply-thread-1')!
    reply.value = 'Updated'
    reply.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    button('Опубликовать').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Updated'))

    button('Закрыть').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Закрыто · local-owner'))
    expect(button('Открыть снова')).not.toBeNull()
  })

  it('requires a persisted project before accepting comments', async () => {
    compositionState.projectId = null
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    component = mount(CompositionReviewPanel, { target })
    await settle()

    expect(target.textContent).toContain('Сохраните композицию')
    expect(target.querySelector('textarea')).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('requires an authenticated session before loading private review data', async () => {
    setAuthSessionForTests(null)
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    component = mount(CompositionReviewPanel, { target })
    await settle()

    expect(target.textContent).toContain('Войти в Review')
    expect(target.querySelector('#review-auth-password')).not.toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('lets the server-known owner assign a durable member role', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if ((init?.method ?? 'GET') === 'GET') {
        const body = path.endsWith('/members') ? [{ actor: 'local-owner', role: 'owner' }] : []
        return new Response(JSON.stringify(body), { status: 200 })
      }
      return new Response(JSON.stringify({ actor: 'editor-1', role: 'commenter' }), { status: 200 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(CompositionReviewPanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Участники · 1'))

    const details = target.querySelector<HTMLDetailsElement>('.composition-review-members')!
    details.open = true
    details.dispatchEvent(new Event('toggle'))
    await settle()
    const input = target.querySelector<HTMLInputElement>('#composition-review-member')!
    input.value = 'editor-1'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    button('Сохранить роль').click()
    await vi.waitFor(() => expect(target.textContent).toContain('editor-1'))

    const call = fetchMock.mock.calls.find(([path, init]) => String(path).endsWith('/members/editor-1') && init?.method === 'PUT')!
    expect(JSON.parse(String(call[1]?.body))).toEqual({ role: 'commenter' })

    button('Передать проект').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Проект передан пользователю editor-1'))
    const transfer = fetchMock.mock.calls.find(([path, init]) => String(path).endsWith('/ownership-transfer') && init?.method === 'POST')!
    expect(JSON.parse(String(transfer[1]?.body))).toEqual({ targetActor: 'editor-1' })
  })
})
