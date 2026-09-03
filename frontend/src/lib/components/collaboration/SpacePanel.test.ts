// @ts-expect-error Vitest SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setAuthSessionForTests } from '$lib/state/auth.svelte.js'
import SpacePanel from './SpacePanel.svelte'

let target: HTMLDivElement
let component: ReturnType<typeof mount>

beforeEach(() => {
  window.history.replaceState(null, '', '/')
  target = document.createElement('div')
  document.body.append(target)
  setAuthSessionForTests({ user: { id: '1', username: 'alice', createdAt: 1 }, token: 'token', expiresAt: 9999999999 })
})

afterEach(async () => {
  if (component) await unmount(component)
  setAuthSessionForTests(null)
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  target.remove()
})

describe('SpacePanel', () => {
  it('accepts an invite from the URL once and removes the bearer secret from browser history', async () => {
    window.history.replaceState(null, '', '/?spaceInvite=secret.part')
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path.includes('/space-invites/secret.part/accept') && init?.method === 'POST') {
        return new Response(JSON.stringify({ id: 'space-2', name: 'Review Team', role: 'viewer', createdAt: 1, updatedAt: 1 }), { status: 200 })
      }
      if (path === '/api/spaces') return new Response(JSON.stringify([
        { id: 'space-2', name: 'Review Team', role: 'viewer', createdAt: 1, updatedAt: 1 },
      ]), { status: 200 })
      if (path.endsWith('/members')) return new Response(JSON.stringify([{ actor: 'alice', role: 'viewer' }]), { status: 200 })
      return new Response('{}', { status: 404 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(SpacePanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Invite принят: Review Team · viewer'))
    expect(fetchMock.mock.calls.filter(([path]) => String(path).includes('/space-invites/secret.part/accept'))).toHaveLength(1)
    expect(window.location.search).toBe('')
  })

  it('creates a seven-day one-time invite link for the selected role', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path === '/api/spaces') return new Response(JSON.stringify([
        { id: 'space-1', name: 'Studio', role: 'owner', createdAt: 1, updatedAt: 1 },
      ]), { status: 200 })
      if (path.endsWith('/members')) return new Response(JSON.stringify([{ actor: 'alice', role: 'owner' }]), { status: 200 })
      if (path.endsWith('/invites') && init?.method === 'POST') {
        return new Response(JSON.stringify({ id: 'invite-1', spaceId: 'space-1', role: 'editor', expiresAt: 99, token: 'secret.part' }), { status: 201 })
      }
      return new Response('{}', { status: 404 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(SpacePanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Studio · owner'))
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Создать invite'))!.click()
    await vi.waitFor(() => expect(target.querySelector<HTMLInputElement>('#space-invite-link')?.value).toContain('spaceInvite=secret.part'))
    const inviteRequest = fetchMock.mock.calls.find(([path, init]) =>
      String(path).endsWith('/spaces/space-1/invites') && init?.method === 'POST')!
    expect(JSON.parse(String(inviteRequest[1]?.body))).toEqual({ role: 'editor', ttlSeconds: 604800 })
  })

  it('creates a space and adds an editor through authenticated APIs', async () => {
    Object.defineProperty(window, 'confirm', { configurable: true, value: vi.fn(() => true) })
    const downloaded: string[] = []
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      downloaded.push(this.getAttribute('href') ?? '')
    })
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path === '/api/spaces' && !init?.method) return new Response('[]', { status: 200 })
      if (path === '/api/spaces') return new Response(JSON.stringify({ id: 'space-1', name: 'Launch', role: 'owner', createdAt: 1, updatedAt: 1 }), { status: 201 })
      if (path === '/api/composition-projects') return new Response(JSON.stringify([
        { id: 'project-1', spaceId: 'space-1', name: 'One', schemaVersion: 2, mode: 'composition', document: {}, sourceIds: [], revision: 1, createdAt: 1, updatedAt: 1 },
        { id: 'other-project', spaceId: 'other', name: 'Other', schemaVersion: 2, mode: 'composition', document: {}, sourceIds: [], revision: 1, createdAt: 1, updatedAt: 1 },
      ]), { status: 200 })
      if (init?.method === 'PATCH') return new Response(JSON.stringify({ id: 'space-1', name: 'Studio', role: 'owner', createdAt: 1, updatedAt: 2 }), { status: 200 })
      if (init?.method === 'DELETE') return new Response(null, { status: 204 })
      return new Response(JSON.stringify({ actor: 'bob', role: 'editor' }), { status: 200 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(SpacePanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Рабочие пространства · 0'))
    const details = target.querySelector('details')!
    details.open = true
    const name = target.querySelector<HTMLInputElement>('#new-space-name')!
    name.value = 'Launch'; name.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Создать'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Launch · owner'))
    const rename = target.querySelector<HTMLInputElement>('#rename-space-name')!
    rename.value = 'Studio'; rename.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Переименовать'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Studio · owner'))
    expect(fetchMock.mock.calls.some(([path, init]) => String(path).endsWith('/spaces/space-1') && init?.method === 'PATCH' && init.body === JSON.stringify({ name: 'Studio', baseUpdatedAt: 1 }))).toBe(true)
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Экспорт всех'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Начат экспорт 1 .veproj'))
    expect(downloaded).toEqual(['/api/composition-projects/project-1/archive'])
    const actor = target.querySelector<HTMLInputElement>('#space-member-name')!
    actor.value = 'bob'; actor.dispatchEvent(new Event('input', { bubbles: true })); await tick()
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Добавить'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('bob'))
    expect(fetchMock.mock.calls.some(([path, init]) => String(path).endsWith('/members/bob') && init?.method === 'PUT')).toBe(true)
    target.querySelector<HTMLButtonElement>('[aria-label="Удалить bob из пространства"]')!.click()
    await vi.waitFor(() => expect(target.textContent).not.toContain('bob'))
    expect(fetchMock.mock.calls.some(([path, init]) => String(path).endsWith('/members/bob') && init?.method === 'DELETE')).toBe(true)
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Удалить пустое'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Рабочие пространства · 0'))
    expect(fetchMock.mock.calls.some(([path, init]) => String(path).endsWith('/spaces/space-1') && init?.method === 'DELETE')).toBe(true)
  })

  it('transfers ownership to an existing member and refreshes roles', async () => {
    Object.defineProperty(window, 'confirm', { configurable: true, value: vi.fn(() => true) })
    let transferred = false
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path.endsWith('/ownership-transfer') && init?.method === 'POST') {
        transferred = true
        return new Response(JSON.stringify({ actor: 'bob', role: 'owner' }), { status: 200 })
      }
      if (path === '/api/spaces') return new Response(JSON.stringify([
        { id: 'space-1', name: 'Studio', role: transferred ? 'editor' : 'owner', createdAt: 1, updatedAt: transferred ? 2 : 1 },
      ]), { status: 200 })
      return new Response(JSON.stringify(transferred
        ? [{ actor: 'alice', role: 'editor' }, { actor: 'bob', role: 'owner' }]
        : [{ actor: 'alice', role: 'owner' }, { actor: 'bob', role: 'editor' }]), { status: 200 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(SpacePanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Studio · owner'))
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Передать')!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Studio · editor'))
    expect(fetchMock.mock.calls.some(([path, init]) => String(path).endsWith('/ownership-transfer')
      && init?.method === 'POST' && init.body === JSON.stringify({ targetActor: 'bob' }))).toBe(true)
    expect(target.querySelector('#space-transfer-owner')).toBeNull()
  })

  it('tears down projects and scoped media before deleting the space', async () => {
    Object.defineProperty(window, 'prompt', { configurable: true, value: vi.fn(() => 'Studio') })
    const mutations: string[] = []
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input)
      if (path === '/api/spaces') return new Response(JSON.stringify([
        { id: 'space-1', name: 'Studio', role: 'owner', createdAt: 1, updatedAt: 1 },
      ]), { status: 200 })
      if (path.endsWith('/members')) return new Response(JSON.stringify([{ actor: 'alice', role: 'owner' }]), { status: 200 })
      if (path === '/api/composition-projects' && !init?.method) return new Response(JSON.stringify([
        { id: 'project-1', spaceId: 'space-1' }, { id: 'project-2', spaceId: 'space-1' },
      ]), { status: 200 })
      if (path === '/api/library' && !init?.method) {
        expect(new Headers(init?.headers).get('X-Space-Id')).toBe('space-1')
        return new Response(JSON.stringify([{ id: 'source-1' }, { id: 'source-2' }]), { status: 200 })
      }
      if (init?.method === 'DELETE') { mutations.push(path); return new Response(null, { status: 204 }) }
      return new Response('{}', { status: 404 })
    })
    vi.stubGlobal('fetch', fetchMock)
    component = mount(SpacePanel, { target })
    await vi.waitFor(() => expect(target.textContent).toContain('Studio · owner'))
    ;[...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Удалить со всем'))!.click()
    await vi.waitFor(() => expect(target.textContent).toContain('Space «Studio» и его содержимое удалены'))
    expect(mutations).toEqual([
      '/api/composition-projects/project-1',
      '/api/composition-projects/project-2',
      '/api/library/source-1',
      '/api/library/source-2',
      '/api/spaces/space-1',
    ])
  })
})
