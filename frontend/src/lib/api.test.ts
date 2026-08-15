import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  ApiError,
  BackendUnavailableError,
  compositionProjectArchiveUrl,
  createLibraryProxy,
  createCompositionProject,
  deleteLibraryProxy,
  getCompositionProject,
  getLibraryProxies,
  getLut,
  getProjects,
  libraryFilmstripUrl,
  libraryThumbnailUrl,
  patchLibraryMetadata,
  pollJob,
  putLibraryMetadata,
  importCompositionProjectArchive,
  renderComposition,
  updateCompositionProject,
  uploadLut,
} from './api'
import { addClip, addTrack, createComposition, registerSource } from './composition/commands'
import { buildCompositionRenderRequest } from './composition/payload'

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

describe('library metadata API', () => {
  it('encodes ids and sends exact PATCH and PUT metadata bodies', async () => {
    const item = {
      id: 'source/one',
      kind: 'source',
      filename: 'one.webm',
      url: '/files/sources/one.webm',
      favorite: true,
      tags: ['client'],
      createdAt: 1,
    }
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(item), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(item), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(patchLibraryMetadata('source/one', { favorite: true })).resolves.toEqual(item)
    await expect(
      putLibraryMetadata('source/one', { title: null, favorite: true, tags: ['client'] }),
    ).resolves.toEqual(item)

    const [patchPath, patchInit] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(patchPath).toBe('/api/library/source%2Fone/metadata')
    expect(patchInit).toMatchObject({
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
    })
    expect(JSON.parse(String(patchInit.body))).toEqual({ favorite: true })

    const [putPath, putInit] = fetchMock.mock.calls[1] as [string, RequestInit]
    expect(putPath).toBe('/api/library/source%2Fone/metadata')
    expect(putInit.method).toBe('PUT')
    expect(JSON.parse(String(putInit.body))).toEqual({
      title: null,
      favorite: true,
      tags: ['client'],
    })
  })
})

describe('library thumbnail URL', () => {
  it('returns only a same-origin stable API URL for a validated media id', () => {
    expect(libraryThumbnailUrl('source_01.webm')).toBe(
      '/api/library/source_01.webm/thumbnail',
    )
    expect(() => libraryThumbnailUrl('../escape')).toThrow(TypeError)
    expect(() => libraryThumbnailUrl('source/one')).toThrow(TypeError)
    expect(() => libraryThumbnailUrl('')).toThrow(TypeError)
  })

  it('uses a separate validated same-origin filmstrip route', () => {
    expect(libraryFilmstripUrl('video-01')).toBe('/api/library/video-01/filmstrip')
    expect(() => libraryFilmstripUrl('../escape')).toThrow(TypeError)
    expect(() => libraryFilmstripUrl('video/one')).toThrow(TypeError)
  })
})

describe('proxy API', () => {
  const key = 'a'.repeat(64)
  const sha256 = 'b'.repeat(64)
  const fingerprint = 'c'.repeat(64)
  const profile = { maxWidth: 720, codec: 'h264' as const, quality: 28, includeAudio: true }

  it('uses encoded source routes and validates POST, GET and DELETE payloads', async () => {
    const list = {
      sourceId: 'source/one',
      sourceFingerprint: fingerprint,
      status: 'ready',
      proxies: [{
        key,
        profile,
        status: 'ready',
        url: `/files/proxies/${key}.mp4`,
        sizeBytes: 1234,
        sha256,
      }],
      jobs: [],
    }
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'job-1', key }), { status: 202 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(list), { status: 200 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(createLibraryProxy('source/one', profile)).resolves.toEqual({ jobId: 'job-1', key })
    await expect(getLibraryProxies('source/one')).resolves.toEqual(list)
    await expect(deleteLibraryProxy('source/one', key)).resolves.toBeUndefined()

    const [createPath, createInit] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(createPath).toBe('/api/library/source%2Fone/proxies')
    expect(createInit.method).toBe('POST')
    expect(JSON.parse(String(createInit.body))).toEqual(profile)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/source%2Fone/proxies')
    expect(fetchMock.mock.calls[2]).toEqual([
      `/api/library/source%2Fone/proxies/${key}`,
      { method: 'DELETE' },
    ])
  })

  it('rejects untrusted proxy media URLs and mismatched source identities', async () => {
    const unsafe = {
      sourceId: 'other-source',
      sourceFingerprint: fingerprint,
      status: 'ready',
      proxies: [{
        key,
        profile,
        status: 'ready',
        url: `https://evil.example/files/proxies/${key}.mp4`,
        sizeBytes: 1,
        sha256,
      }],
      jobs: [],
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(unsafe), { status: 200 })))

    await expect(getLibraryProxies('source-one')).rejects.toThrow('list.sourceId')
  })

  it('rejects malformed delete keys before issuing a request', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    await expect(deleteLibraryProxy('source-one', '../escape')).rejects.toThrow('proxy')
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

describe('composition API', () => {
  function document() {
    let value = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    value = registerSource(value, {
      id: 'source-video',
      kind: 'video',
      durationTicks: 2_000_000,
      width: 1280,
      height: 720,
      hasAudio: true,
    })
    value = addTrack(value, {
      id: 'video-track',
      kind: 'video',
      name: 'Видео 1',
      locked: false,
      hidden: false,
      muted: false,
      clips: [],
    })
    return addClip(value, 'video-track', {
      id: 'video-clip',
      kind: 'video',
      sourceId: 'source-video',
      timelineStartTicks: 0,
      sourceInTicks: 0,
      sourceOutTicks: 2_000_000,
      transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
      opacity: 1,
      sourceAudioEnabled: true,
      audioGain: 1,
    })
  }

  it('posts the exact render endpoint and project envelopes', async () => {
    const composition = document()
    const project = {
      id: 'project-1',
      name: 'Монтаж',
      schemaVersion: 2,
      mode: 'composition',
      document: composition,
      sourceIds: ['source-video'],
      createdAt: 1,
      updatedAt: 1,
    }
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'job-1' }), { status: 202 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(project), { status: 201 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(project), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(renderComposition(buildCompositionRenderRequest(composition))).resolves.toEqual({ jobId: 'job-1' })
    await expect(createCompositionProject({ name: 'Монтаж', document: composition })).resolves.toEqual(project)
    await expect(updateCompositionProject('project/one', { document: composition })).resolves.toEqual(project)

    const [renderPath, renderInit] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(renderPath).toBe('/api/compositions/render')
    expect(renderInit.method).toBe('POST')
    expect(JSON.parse(String(renderInit.body))).toMatchObject({
      schemaVersion: 1,
      output: {
        profile: { container: 'mp4', codec: 'h264' },
        qualityTier: 'medium',
      },
    })

    const [createPath, createInit] = fetchMock.mock.calls[1] as [string, RequestInit]
    expect(createPath).toBe('/api/composition-projects')
    expect(JSON.parse(String(createInit.body))).toEqual({
      schemaVersion: 2,
      mode: 'composition',
      name: 'Монтаж',
      document: composition,
    })
    const [updatePath, updateInit] = fetchMock.mock.calls[2] as [string, RequestInit]
    expect(updatePath).toBe('/api/composition-projects/project%2Fone')
    expect(updateInit.method).toBe('PUT')
  })

  it('maps a missing composition project to null', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 404 })))
    await expect(getCompositionProject('missing')).resolves.toBeNull()
  })

  it('builds an encoded direct archive URL so the browser can stream it to disk', () => {
    expect(compositionProjectArchiveUrl('project/one')).toBe(
      '/api/composition-projects/project%2Fone/archive',
    )
  })

  it('imports a portable project as multipart without overriding its content type', async () => {
    const composition = document()
    const imported = {
      project: {
        id: 'imported-project',
        name: 'Portable',
        schemaVersion: 2,
        mode: 'composition',
        document: composition,
        sourceIds: ['new-source'],
        createdAt: 2,
        updatedAt: 2,
      },
      sourceMapping: { 'old-source': 'new-source' },
    }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(imported), { status: 201 }))
    vi.stubGlobal('fetch', fetchMock)
    const file = new File(['VEPROJ\r\n'], 'portable.veproj', {
      type: 'application/vnd.video-editor.project',
    })

    await expect(importCompositionProjectArchive(file)).resolves.toEqual(imported)

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/composition-projects/import')
    expect(init.method).toBe('POST')
    expect(init.headers).toBeUndefined()
    expect(init.body).toBeInstanceOf(FormData)
    expect((init.body as FormData).get('file')).toBe(file)
  })
})
