import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, getProjects } from './api'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('API error boundary', () => {
  it('preserves the backend message, status and machine code', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ error: 'Внутренняя ошибка сервера', code: 'internal_error' }),
          { status: 500, headers: { 'content-type': 'application/json' } },
        ),
      ),
    )

    await expect(getProjects()).rejects.toMatchObject({
      name: 'ApiError',
      message: 'Внутренняя ошибка сервера',
      status: 500,
      code: 'internal_error',
    })
  })

  it('keeps plain text as a compatibility fallback', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('proxy failed', { status: 502 })))

    await expect(getProjects()).rejects.toMatchObject({
      message: 'proxy failed',
      status: 502,
      code: undefined,
    })
  })

  it('uses the backend-down message only for a network exception', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('connection refused')))

    const request = getProjects()
    await expect(request).rejects.toThrow('Сервер недоступен')
    await expect(request).rejects.not.toBeInstanceOf(ApiError)
  })
})
