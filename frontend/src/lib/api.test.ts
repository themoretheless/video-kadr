import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, BackendUnavailableError, getLut, getProjects, pollJob, uploadLut } from './api'

const TEST_LUT_ID = '11111111-1111-4111-8111-111111111111'

afterEach(() => {
  vi.useRealTimers()
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
    await expect(request).rejects.toBeInstanceOf(BackendUnavailableError)
    await expect(request).rejects.not.toBeInstanceOf(ApiError)
  })
})

describe('LUT upload', () => {
  it('posts the cube as multipart data and returns its metadata', async () => {
    const asset = {
      id: TEST_LUT_ID,
      name: 'Cinema',
      cubeSize: 33,
      sizeBytes: 1024,
      sha256: 'deadbeef',
    }
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(asset), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const file = new File(['LUT_3D_SIZE 2'], 'cinema.cube', { type: 'text/plain' })
    const controller = new AbortController()

    await expect(uploadLut(file, controller.signal)).resolves.toEqual(asset)
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/luts')
    expect(init.method).toBe('POST')
    expect(init.signal).toBe(controller.signal)
    expect(init.body).toBeInstanceOf(FormData)
    expect((init.body as FormData).get('file')).toBe(file)
    expect(init.headers).toBeUndefined()
  })

  it('resolves stored LUT metadata by encoded id', async () => {
    const asset = { id: TEST_LUT_ID, name: 'Cinema', cubeSize: 33, sizeBytes: 1024 }
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(asset), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    await expect(getLut(TEST_LUT_ID)).resolves.toEqual(asset)
    expect(fetchMock).toHaveBeenCalledWith(`/api/luts/${TEST_LUT_ID}`, undefined)
  })

  it('surfaces the structured backend validation error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: 'Некорректный LUT', code: 'invalid_lut' }), {
          status: 422,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    )

    await expect(uploadLut(new File(['bad'], 'bad.cube'))).rejects.toMatchObject({
      name: 'ApiError',
      message: 'Некорректный LUT',
      status: 422,
      code: 'invalid_lut',
    })
  })
})

describe('job polling', () => {
  it('waits exactly one interval between non-terminal and terminal states', async () => {
    vi.useFakeTimers()
    const running = { id: 'job-1', status: 'running', progress: 25 }
    const done = { id: 'job-1', status: 'done', result: { id: 'result-1' } }
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(running), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(done), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    const ticks: string[] = []

    const pending = pollJob('job-1', (job) => ticks.push(job.status))
    await vi.advanceTimersByTimeAsync(0)
    expect(fetchMock).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(499)
    expect(fetchMock).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(1)

    await expect(pending).resolves.toMatchObject({ status: 'done' })
    expect(ticks).toEqual(['running', 'done'])
    expect(vi.getTimerCount()).toBe(0)
  })

  it.each([
    ['cancelled', undefined, 'cancelled'],
    ['interrupted', undefined, 'Задача прервана'],
    ['error', 'render failed', 'render failed'],
  ] as const)('rejects terminal %s jobs without scheduling another tick', async (status, error, message) => {
    vi.useFakeTimers()
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ id: 'job-1', status, error }), { status: 200 }),
      ),
    )

    await expect(pollJob('job-1')).rejects.toThrow(message)
    expect(vi.getTimerCount()).toBe(0)
  })
})
