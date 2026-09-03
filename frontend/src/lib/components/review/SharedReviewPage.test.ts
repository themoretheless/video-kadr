// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import SharedReviewPage from './SharedReviewPage.svelte'

let target: HTMLDivElement
afterEach(() => { vi.unstubAllGlobals(); target?.remove() })

describe('SharedReviewPage', () => {
  it('renders read-only timestamped threads without exposing project media', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      projectId: 'project-1', projectName: 'Client cut', expiresAt: 2_000_000_000,
      threads: [{ id: 'thread-1', projectId: 'project-1', resolvedAt: null, comments: [
        { id: 'comment-1', author: 'alice', body: 'Tighten this cut', timelineTick: 62_500_000, createdAt: 1 },
      ] }],
    }), { status: 200 })))
    target = document.createElement('div'); document.body.append(target)
    const component = mount(SharedReviewPage, { target, props: { token: 'safe-token.value' } })
    await vi.waitFor(() => expect(target.textContent).toContain('Client cut'))

    expect(target.textContent).toContain('1:02.500')
    expect(target.textContent).toContain('Tighten this cut')
    expect(target.textContent).toContain('Только просмотр')
    expect(target.querySelector('textarea, input, video')).toBeNull()
    expect(fetch).toHaveBeenCalledWith('/api/review-shares/safe-token.value', undefined)
    await unmount(component)
  })

  it('explains an expired or revoked link and allows retry', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'not found' }), {
      status: 404, headers: { 'content-type': 'application/json' },
    }))
    vi.stubGlobal('fetch', fetchMock)
    target = document.createElement('div'); document.body.append(target)
    const component = mount(SharedReviewPage, { target, props: { token: 'expired-token' } })
    await vi.waitFor(() => expect(target.textContent).toContain('истекла или была отозвана'))
    target.querySelector<HTMLButtonElement>('button')?.click(); await tick()
    expect(fetchMock).toHaveBeenCalledTimes(2)
    await unmount(component)
  })
})
