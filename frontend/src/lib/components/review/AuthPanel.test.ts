// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AuthPanel from './AuthPanel.svelte'
import { authState, setAuthSessionForTests } from '$lib/state/auth.svelte.js'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  setAuthSessionForTests(null)
  vi.unstubAllGlobals()
  target?.remove()
})

describe('AuthPanel', () => {
  it('registers and accepts only the server-issued identity and bearer token', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({
      user: { id: 'server-user-id', username: 'alice', createdAt: 1 },
      token: 'server-issued-token', expiresAt: 2_000_000_000,
    }), { status: 201 }))
    vi.stubGlobal('fetch', fetchMock)
    target = document.createElement('div'); document.body.append(target)
    component = mount(AuthPanel, { target })

    const toggle = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Регистрация'))!
    toggle.click(); await tick()
    const username = target.querySelector<HTMLInputElement>('#review-auth-username')!
    const password = target.querySelector<HTMLInputElement>('#review-auth-password')!
    username.value = 'alice'; username.dispatchEvent(new Event('input', { bubbles: true }))
    password.value = 'correct horse battery staple'; password.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    const submit = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Создать и войти'))!
    submit.click()

    await vi.waitFor(() => expect(authState.user?.username).toBe('alice'))
    expect(authState.token).toBe('server-issued-token')
    expect(fetchMock).toHaveBeenCalledWith('/api/auth/register', expect.objectContaining({ method: 'POST' }))
  })
})
