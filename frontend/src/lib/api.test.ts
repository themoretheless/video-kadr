import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  analyzeScopesBinary,
  getAudioDspCatalog,
  ApiError,
  BackendUnavailableError,
  beginYouTubeConnection,
  disconnectYouTube,
  publishYouTube,
  compositionProjectArchiveUrl,
  createProjectReviewThread,
  createLibraryProxy,
  createCompositionProject,
  createSpace,
  createSpaceInvite,
  createSpaceTemplate,
  deleteSpaceTemplate,
  deleteLibraryProxy,
  getCompositionProject,
  getCompositionProjectIfChanged,
  getCompositionProjectsIfChanged,
  getSpaces,
  getYouTubeConnectionStatus,
  getSpaceTemplates,
  getSpaceMembers,
  acceptSpaceInvite,
  getSpaceBrandKit,
  getProjectReviewThreads,
  getProjectReviewMembers,
  getProjectReviewAudit,
  getSharedReview,
  getLibraryProxies,
  getLibrary,
  getLut,
  getProjects,
  libraryFilmstripUrl,
  libraryThumbnailUrl,
  loginAuthUser,
  logoutAuthSession,
  patchLibraryMetadata,
  pollJob,
  putLibraryMetadata,
  importCompositionProjectArchive,
  renderComposition,
  registerAuthUser,
  replyToProjectReviewThread,
  setProjectReviewThreadResolved,
  setProjectReviewMember,
  setSpaceMember,
  searchStockCatalog,
  subscribeCompositionProjectChanges,
  transferCompositionProjectOwnership,
  getAuthSession,
  updateCompositionProject,
  updateSpaceTemplate,
  updateSpaceBrandKit,
  uploadLut,
  sampleAnimatedColors,
} from './api'
import { addClip, addTrack, createComposition, registerSource } from './composition/commands'
import { buildCompositionRenderRequest } from './composition/payload'
import { createCompositionTemplate } from './composition/templates'

const TEST_LUT_ID = '11111111-1111-4111-8111-111111111111'

it('transports native color values and exact microsecond samples without scalar coercion', async () => {
  const request = { track: { keys: [{ t_us: -250000,value: { r: 255,g: 0,b: 0,a: 128 },in_: [2/3,2/3] as const,out: [1/3,1/3] as const,segment: { kind: 'Linear' as const } }],extrapolate: { before: 'Offset' as const,after: 'PingPong' as const } },default: { r: 0,g: 0,b: 0,a: 255 },times_us: [-1,0,250000] }
  const response = { colors: [{ r: 255,g: 0,b: 0,a: 128 },{ r: 255,g: 0,b: 0,a: 128 },{ r: 255,g: 0,b: 0,a: 128 }] }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(response)))
  vi.stubGlobal('fetch',fetchMock)
  const signal = new AbortController().signal
  await expect(sampleAnimatedColors(request,signal)).resolves.toEqual(response)
  expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/animated-color')
  const init = fetchMock.mock.calls[0]![1] as RequestInit
  expect(JSON.parse(init.body as string)).toEqual(request)
  expect(init.signal).toBe(signal)
})

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

describe('authentication API', () => {
  it('uses credentials only for register/login and bearer tokens for session/logout', async () => {
    const session = { user: { id: 'u1', username: 'alice', createdAt: 1 }, token: 'secret', expiresAt: 2 }
    const fetchMock = vi.fn().mockImplementation(async (path: string) => path === '/api/auth/logout'
      ? new Response(null, { status: 204 })
      : new Response(JSON.stringify(session), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await registerAuthUser('alice', 'long secure password')
    await loginAuthUser('alice', 'long secure password')
    await getAuthSession('opaque-token')
    await logoutAuthSession('opaque-token')

    expect(fetchMock.mock.calls.map(([path]) => path)).toEqual([
      '/api/auth/register', '/api/auth/login', '/api/auth/session', '/api/auth/logout',
    ])
    expect(JSON.parse(String((fetchMock.mock.calls[0][1] as RequestInit).body))).toEqual({
      username: 'alice', password: 'long secure password',
    })
    for (const index of [2, 3]) {
      expect((fetchMock.mock.calls[index][1] as RequestInit).headers).toEqual({ Authorization: 'Bearer opaque-token' })
    }
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

  it('sends bearer and selected Space headers for scoped library operations', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response('[]', { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'source-1' }), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await getLibrary('session-token', 'space-1')
    await patchLibraryMetadata('source-1', { favorite: true }, 'session-token', 'space-1')

    expect(fetchMock.mock.calls[0]?.[1]).toMatchObject({
      headers: { Authorization: 'Bearer session-token', 'X-Space-Id': 'space-1' },
    })
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({
      headers: {
        Authorization: 'Bearer session-token',
        'X-Space-Id': 'space-1',
        'Content-Type': 'application/json',
      },
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
      revision: 7,
      createdAt: 1,
      updatedAt: 1,
    }
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'job-1' }), { status: 202 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(project), { status: 201 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(project), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(renderComposition(buildCompositionRenderRequest(composition), 'render-token')).resolves.toEqual({ jobId: 'job-1' })
    await expect(createCompositionProject({ name: 'Монтаж', spaceId: 'space-1', document: composition }, 'session-token')).resolves.toEqual(project)
    await expect(updateCompositionProject('project/one', { document: composition, baseRevision: 7 }, 'session-token')).resolves.toEqual(project)

    const [renderPath, renderInit] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(renderPath).toBe('/api/compositions/render')
    expect(renderInit.method).toBe('POST')
    expect(new Headers(renderInit.headers).get('Authorization')).toBe('Bearer render-token')
    expect(JSON.parse(String(renderInit.body))).toMatchObject({
      schemaVersion: 1,
      output: {
        profile: { container: 'mp4', codec: 'h264' },
        qualityTier: 'medium',
      },
    })

    const [createPath, createInit] = fetchMock.mock.calls[1] as [string, RequestInit]
    expect(createPath).toBe('/api/composition-projects')
    expect(createInit.headers).toEqual({ Authorization: 'Bearer session-token', 'Content-Type': 'application/json' })
    expect(JSON.parse(String(createInit.body))).toEqual({
      schemaVersion: 2,
      mode: 'composition',
      name: 'Монтаж',
      spaceId: 'space-1',
      document: composition,
    })
    const [updatePath, updateInit] = fetchMock.mock.calls[2] as [string, RequestInit]
    expect(updatePath).toBe('/api/composition-projects/project%2Fone')
    expect(updateInit.method).toBe('PUT')
    expect(updateInit.headers).toEqual({ Authorization: 'Bearer session-token', 'Content-Type': 'application/json' })
    expect(JSON.parse(String(updateInit.body))).toMatchObject({ baseRevision: 7 })
  })

  it('maps a missing composition project to null', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 404 })))
    await expect(getCompositionProject('missing', 'session-token')).resolves.toBeNull()
  })

  it('uses the project revision ETag and maps 304 to unchanged', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 304 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      getCompositionProjectIfChanged('project/one', 'session-token', 7),
    ).resolves.toBeUndefined()
    expect(fetchMock).toHaveBeenCalledWith('/api/composition-projects/project%2Fone', {
      headers: {
        Authorization: 'Bearer session-token',
        'If-None-Match': '"revision-7"',
      },
    })
  })

  it('conditionally refreshes the visible project list with its aggregate ETag', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response('[]', {
        status: 200,
        headers: { etag: '"projects-hash"', 'content-type': 'application/json' },
      }))
      .mockResolvedValueOnce(new Response(null, { status: 304 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(getCompositionProjectsIfChanged('session-token', null)).resolves.toEqual({
      etag: '"projects-hash"',
      projects: [],
    })
    await expect(
      getCompositionProjectsIfChanged('session-token', '"projects-hash"'),
    ).resolves.toBeUndefined()
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({
      headers: {
        Authorization: 'Bearer session-token',
        'If-None-Match': '"projects-hash"',
      },
    })
  })

  it('subscribes to project and resync SSE events with credentials', () => {
    const addEventListener = vi.fn()
    const close = vi.fn()
    const EventSourceMock = vi.fn(function () {
      return { addEventListener, close }
    })
    vi.stubGlobal('EventSource', EventSourceMock)
    const onChange = vi.fn()

    const unsubscribe = subscribeCompositionProjectChanges(onChange)
    expect(EventSourceMock).toHaveBeenCalledWith('/api/composition-projects/events', {
      withCredentials: true,
    })
    expect(addEventListener).toHaveBeenCalledWith('project', onChange)
    expect(addEventListener).toHaveBeenCalledWith('resync', onChange)
    unsubscribe()
    expect(close).toHaveBeenCalledOnce()
  })

  it('routes the complete review thread lifecycle with encoded identifiers', async () => {
    const thread = {
      id: 'thread/one',
      projectId: 'project/one',
      comments: [{ id: 'comment-1', author: 'alice', body: 'Check cut', timelineTick: 90_000, createdAt: 1 }],
    }
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify(thread), { status: 200, headers: { 'content-type': 'application/json' } }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const token = 'alice-session'

    await getProjectReviewThreads('project/one', token)
    await createProjectReviewThread('project/one', token, 'Check cut', 90_000)
    await replyToProjectReviewThread('thread/one', token, 'Updated')
    await setProjectReviewThreadResolved('thread/one', token, true)
    await getProjectReviewMembers('project/one', token)
    await setProjectReviewMember('project/one', token, 'bob', 'commenter')
    await transferCompositionProjectOwnership('project/one', token, 'bob')
    await getProjectReviewAudit('project/one', token)

    expect(fetchMock.mock.calls.map(([path]) => path)).toEqual([
      '/api/composition-projects/project%2Fone/reviews',
      '/api/composition-projects/project%2Fone/reviews',
      '/api/review-threads/thread%2Fone/replies',
      '/api/review-threads/thread%2Fone/resolution',
      '/api/composition-projects/project%2Fone/members',
      '/api/composition-projects/project%2Fone/members/bob',
      '/api/composition-projects/project%2Fone/ownership-transfer',
      '/api/composition-projects/project%2Fone/review-audit',
    ])
    expect(JSON.parse(String((fetchMock.mock.calls[1][1] as RequestInit).body))).toEqual({
      body: 'Check cut', timelineTick: 90_000,
    })
    expect((fetchMock.mock.calls[3][1] as RequestInit).method).toBe('PUT')
    expect(JSON.parse(String((fetchMock.mock.calls[5][1] as RequestInit).body))).toEqual({ role: 'commenter' })
    expect(JSON.parse(String((fetchMock.mock.calls[6][1] as RequestInit).body))).toEqual({ targetActor: 'bob' })
    for (const [, init] of fetchMock.mock.calls) {
      expect((init as RequestInit).headers).toMatchObject({ Authorization: 'Bearer alice-session' })
    }
  })

  it('opens a shared review through its encoded opaque token', async () => {
    const shared = { projectId: 'project-1', projectName: 'Cut', expiresAt: 2, threads: [] }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(shared), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(getSharedReview('token.part/value')).resolves.toEqual(shared)
    expect(fetchMock).toHaveBeenCalledWith('/api/review-shares/token.part%2Fvalue', undefined)
  })

  it('routes authenticated space creation and membership', async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(new Response(JSON.stringify([]), { status: 200 })))
    vi.stubGlobal('fetch', fetchMock)
    await getSpaces('space-token')
    await createSpace('space-token', 'Launch Team')
    await getSpaceMembers('space/one', 'space-token')
    await setSpaceMember('space/one', 'space-token', 'bob', 'editor')
    await createSpaceInvite('space/one', 'space-token', 'viewer', 3600)
    await acceptSpaceInvite('invite/token', 'space-token')
    const template = createCompositionTemplate('team-template', 'Team', document(), [])
    await getSpaceTemplates('space/one', 'space-token')
    await createSpaceTemplate('space/one', 'space-token', template)
    await updateSpaceTemplate('space/one', 'shared/one', 'space-token', 3, template)
    await deleteSpaceTemplate('space/one', 'shared/one', 'space-token')
    await getSpaceBrandKit('space/one', 'space-token')
    await updateSpaceBrandKit('space/one', 'space-token', 2, {
      colors: [{ name: 'Primary', value: '#3366ff' }], fonts: ['Noto Sans'], logoSourceIds: [],
    })
    expect(fetchMock.mock.calls.map(([path]) => path)).toEqual([
      '/api/spaces',
      '/api/spaces',
      '/api/spaces/space%2Fone/members',
      '/api/spaces/space%2Fone/members/bob',
      '/api/spaces/space%2Fone/invites',
      '/api/space-invites/invite%2Ftoken/accept',
      '/api/spaces/space%2Fone/templates',
      '/api/spaces/space%2Fone/templates',
      '/api/spaces/space%2Fone/templates/shared%2Fone',
      '/api/spaces/space%2Fone/templates/shared%2Fone',
      '/api/spaces/space%2Fone/brand-kit',
      '/api/spaces/space%2Fone/brand-kit',
    ])
    expect(JSON.parse(String((fetchMock.mock.calls[1][1] as RequestInit).body))).toEqual({ name: 'Launch Team' })
    expect(JSON.parse(String((fetchMock.mock.calls[3][1] as RequestInit).body))).toEqual({ role: 'editor' })
    expect(JSON.parse(String((fetchMock.mock.calls[4][1] as RequestInit).body))).toEqual({ role: 'viewer', ttlSeconds: 3600 })
    expect(JSON.parse(String((fetchMock.mock.calls[7][1] as RequestInit).body))).toEqual({ template })
    expect(JSON.parse(String((fetchMock.mock.calls[8][1] as RequestInit).body))).toEqual({ baseRevision: 3, template })
    expect((fetchMock.mock.calls[9][1] as RequestInit).method).toBe('DELETE')
    expect(JSON.parse(String((fetchMock.mock.calls[11][1] as RequestInit).body))).toEqual({
      baseRevision: 2,
      kit: { colors: [{ name: 'Primary', value: '#3366ff' }], fonts: ['Noto Sans'], logoSourceIds: [] },
    })
    for (const [, init] of fetchMock.mock.calls) {
      expect((init as RequestInit).headers).toMatchObject({ Authorization: 'Bearer space-token' })
    }
  })

  it('builds an encoded direct archive URL so the browser can stream it to disk', () => {
    expect(compositionProjectArchiveUrl('project/one')).toBe(
      '/api/composition-projects/project%2Fone/archive',
    )
  })

  it('encodes authenticated Pexels stock search filters', async () => {
    const result = { provider: 'Pexels', providerUrl: 'https://www.pexels.com', page: 2, totalResults: 0, assets: [] }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    await expect(searchStockCatalog('stock-token', 'city & night', 'video', 'portrait', 2)).resolves.toEqual(result)
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/stock/search?q=city+%26+night&kind=video&page=2&orientation=portrait',
      { headers: { Authorization: 'Bearer stock-token' } },
    )
  })

  it('starts and disconnects YouTube OAuth without exposing credentials in the request', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ configured: true, connected: false }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ authorizationUrl: 'https://accounts.google.com/oauth' }), { status: 200 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await expect(getYouTubeConnectionStatus('publish-token')).resolves.toEqual({ configured: true, connected: false })
    await expect(beginYouTubeConnection('publish-token')).resolves.toEqual({ authorizationUrl: 'https://accounts.google.com/oauth' })
    await expect(disconnectYouTube('publish-token')).resolves.toBeUndefined()
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'publish-job' }), { status: 202 }))
    await expect(publishYouTube('publish-token', {
      outputId: 'output-1', title: 'Release', description: '', privacyStatus: 'private',
    })).resolves.toEqual({ jobId: 'publish-job' })
    expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/publish/youtube/status', {
      headers: { Authorization: 'Bearer publish-token' },
    })
    expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/publish/youtube/connect', {
      method: 'POST', headers: { Authorization: 'Bearer publish-token', 'Content-Type': 'application/json' }, body: '{}',
    })
    expect(fetchMock).toHaveBeenNthCalledWith(3, '/api/publish/youtube/connect', {
      method: 'DELETE', headers: { Authorization: 'Bearer publish-token' },
    })
    expect(fetchMock).toHaveBeenNthCalledWith(4, '/api/publish/youtube', {
      method: 'POST', headers: { Authorization: 'Bearer publish-token', 'Content-Type': 'application/json' },
      body: JSON.stringify({ outputId: 'output-1', title: 'Release', description: '', privacyStatus: 'private' }),
    })
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

    await expect(importCompositionProjectArchive(file, 'session-token', 'space-1')).resolves.toEqual(imported)

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/composition-projects/import')
    expect(init.method).toBe('POST')
    expect(init.headers).toEqual({ Authorization: 'Bearer session-token', 'X-Space-Id': 'space-1' })
    expect(init.body).toBeInstanceOf(FormData)
    expect((init.body as FormData).get('file')).toBe(file)
  })
})

it('transports nested grouping through native API without rewriting clocks', async () => {
  const { groupNestedComposition } = await import('./api')
  const { addTrack,addClip,createComposition } = await import('./composition/commands')
  const { createSolidColorClip } = await import('./composition/solidColor')
  let document = createComposition({ width: 64,height: 64,fps: 30,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 'colors',kind: 'color',name: 'Color',hidden: false,locked: false,clips: [] })
  document = addClip(document,'colors',createSolidColorClip(document.canvas,'solid',500000,1000000))
  const project = { schemaVersion: 1 as const,root: 'root',compositions: { root: { content: buildCompositionRenderRequest(document).composition,instances: [] } } }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(project)))
  vi.stubGlobal('fetch',fetchMock)
  const signal = new AbortController().signal
  await expect(groupNestedComposition(project,'root','child','group',signal)).resolves.toEqual(project)
  expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/nested-group')
  const init = fetchMock.mock.calls[0]![1] as RequestInit
  expect(JSON.parse(init.body as string)).toEqual({ project,parent: 'root',child: 'child',instanceId: 'group' })
  expect(init.signal).toBe(signal)
})

it('loads native nested RGBA preview with auth and rejects incomplete frames', async () => {
  const { renderNestedPreview } = await import('./api')
  const { createComposition, addTrack, addClip } = await import('./composition/commands')
  const { createSolidColorClip } = await import('./composition/solidColor')
  let document = createComposition({ width: 64, height: 64, fps: 30, backgroundColor: '#000000' })
  document = addTrack(document, { id: 'colors', kind: 'color', name: 'Color', hidden: false, locked: false, clips: [] })
  document = addClip(document, 'colors', createSolidColorClip(document.canvas, 'solid', 0, 1000000))
  const project = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content: buildCompositionRenderRequest(document).composition, instances: [] } } }
  const fetchMock = vi.fn().mockResolvedValue(new Response(new Uint8Array([255, 0, 0, 128]), { headers: { 'x-frame-width': '1', 'x-frame-height': '1' } }))
  vi.stubGlobal('fetch', fetchMock)
  const signal = new AbortController().signal
  const result = await renderNestedPreview(project, 500000, { root: 'output' }, 'session', signal)
  expect(result).toEqual({ width: 1, height: 1, rgba: new Uint8ClampedArray([255, 0, 0, 128]) })
  const init = fetchMock.mock.calls[0]![1] as RequestInit
  expect(init.headers).toEqual({ Authorization: 'Bearer session', 'Content-Type': 'application/json' })
  expect(init.signal).toBe(signal)
  expect(JSON.parse(init.body as string)).toEqual({ project, timeUs: 500000, renderedOutputs: { root: 'output' } })
  fetchMock.mockResolvedValue(new Response(new Uint8Array([0, 0, 255, 255]), { headers: { 'x-frame-width': '1', 'x-frame-height': '1' } }))
  await renderNestedPreview(project, 100000, { child: 'movie' }, 'session', signal, 'child')
  expect(JSON.parse((fetchMock.mock.calls[1]![1] as RequestInit).body as string).compositionId).toBe('child')
  fetchMock.mockResolvedValue(new Response(new Uint8Array([255]), { headers: { 'x-frame-width': '1', 'x-frame-height': '1' } }))
  await expect(renderNestedPreview(project, 0, {}, 'session')).rejects.toThrow('Неполный RGBA')
})

it('sends explicit video-only native export with owner auth and cancellable observation', async () => {
  const { exportNestedVideo, nestedDurations, getJob } = await import('./api')
  const { createComposition, addTrack, addClip } = await import('./composition/commands')
  const { createSolidColorClip } = await import('./composition/solidColor')
  let document = createComposition({ width: 64, height: 64, fps: 30, backgroundColor: '#000000' })
  document = addTrack(document, { id: 'colors', kind: 'color', name: 'Color', hidden: false, locked: false, clips: [] })
  document = addClip(document, 'colors', createSolidColorClip(document.canvas, 'solid', 0, 1000000))
  const project = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content: buildCompositionRenderRequest(document).composition, instances: [] } } }
  const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'job', mode: 'video_only', container: 'mkv' }))).mockResolvedValueOnce(new Response(JSON.stringify({ root: 1000000 }))).mockResolvedValueOnce(new Response(JSON.stringify({ id: 'job', status: 'running' })))
  vi.stubGlobal('fetch', fetchMock)
  const signal = new AbortController().signal
  await exportNestedVideo(project, 'root', 250000, 750000, { child: 'output' }, 'owner', signal)
  expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/nested-export')
  const init = fetchMock.mock.calls[0]![1] as RequestInit
  expect(init.headers).toEqual({ Authorization: 'Bearer owner', 'Content-Type': 'application/json' })
  expect(JSON.parse(init.body as string)).toEqual({ schemaVersion: 1, mode: 'video_only', project, compositionId: 'root', startUs: 250000, endUs: 750000, renderedOutputs: { child: 'output' } })
  await expect(nestedDurations(project, signal)).resolves.toEqual({ root: 1000000 })
  await getJob('job', signal)
  expect((fetchMock.mock.calls[2]![1] as RequestInit).signal).toBe(signal)
})

it('sends scope RGBA bytes without JSON expansion or unrelated buffer data',async()=>{
 const response={samples:[1,1],matrix:'Bt709',unit:'%',stats:null}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(response)))
 vi.stubGlobal('fetch',fetchMock)
 const rgba=new Uint8ClampedArray([1,2,3,4,5,6,7,8]).subarray(2,6)
 const signal=new AbortController().signal
 await expect(analyzeScopesBinary({width:1,height:1,rgba},{scopes:['histogram'],matrix:'Bt601'},signal)).resolves.toEqual(response)
 expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/compositions/scopes-rgba')
 const init=fetchMock.mock.calls[0]?.[1] as RequestInit
 expect(init.headers).toEqual({'Content-Type':'application/octet-stream','X-Frame-Width':'1','X-Frame-Height':'1','X-Scope-Options':'{"scopes":["histogram"],"matrix":"Bt601"}'})
 expect(new Uint8Array(init.body as ArrayBuffer)).toEqual(new Uint8Array([3,4,5,6]))
 expect(init.signal).toBe(signal)
})


describe('native audio DSP catalog transport', () => {
  it('preserves registry choices and forwards cancellation', async () => {
    const catalog = { schemaVersion: 1, effects: [{ id: 'amplify', name: 'Amplify', category: 'channel', params: [{ id: 'gain', name: 'Gain', min: -96, max: 24, default: 0, unit: 'decibels', logScale: false, choices: [] }] }] }
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(catalog), { status: 200, headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetcher)
    const controller = new AbortController()
    expect(await getAudioDspCatalog(controller.signal)).toEqual(catalog)
    expect(String(fetcher.mock.calls[0]?.[0])).toContain('/api/compositions/audio-dsp-catalog')
    expect(fetcher.mock.calls[0]?.[1]?.signal).toBe(controller.signal)
  })
})

it('transports Filmcraft source text exactly and downloads authenticated PCM', async () => {
  const { importFilmcraftProject, exportFilmcraftAudio } = await import('./api')
  const document = '{"largeId":18446744073709551615,"tick":9007199254740993}'
  const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, originalSchema: 12, sequences: [{ id: '18446744073709551615', settings: {} }] })))
    .mockResolvedValueOnce(new Response(new Uint8Array([82, 73, 70, 70]), { headers: { 'Content-Type': 'audio/wav' } }))
  vi.stubGlobal('fetch', fetchMock)
  const signal = new AbortController().signal
  const result = await importFilmcraftProject(document, signal)
  expect(result.documentText).toBe(document)
  expect(JSON.parse((fetchMock.mock.calls[0]![1] as RequestInit).body as string).document).toBe(document)
  const input = { document, sequenceId: '18446744073709551615', startSample: 0, frames: 65, channels: 2 as const, bits: 24 as const, format: 'wav' as const, sources: {} }
  const blob = await exportFilmcraftAudio(input, 'session', signal)
  expect(blob.size).toBe(4)
  expect(fetchMock.mock.calls[1]![0]).toBe('/api/compositions/filmcraft/export-audio')
  const init = fetchMock.mock.calls[1]![1] as RequestInit
  expect(init.headers).toEqual({ Authorization: 'Bearer session', 'Content-Type': 'application/json' })
  expect(init.signal).toBe(signal)
  expect(JSON.parse(init.body as string).sequenceId).toBe(input.sequenceId)
  await expect(exportFilmcraftAudio({ ...input, startSample: Number.MAX_SAFE_INTEGER + 1 })).rejects.toThrow('Недопустимый диапазон')
  expect(fetchMock).toHaveBeenCalledTimes(2)
})

it('sends source trim IDs and frame or tick deltas as exact strings', async () => {
  const { trimFilmcraftProject } = await import('./api')
  const text = '{"tick":9007199254740993}'
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ documentText: text, appliedDeltaTicks: '-9007199254740993' })))
  vi.stubGlobal('fetch', fetchMock)
  const signal = new AbortController().signal
  const input = { document: text, sequenceId: '18446744073709551615', clipId: '9007199254740993', edge: 'in' as const, deltaTicks: '-9007199254740993', linked: true }
  expect(await trimFilmcraftProject(input, signal)).toEqual({ documentText: text, appliedDeltaTicks: input.deltaTicks })
  expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/trim')
  const init = fetchMock.mock.calls[0]![1] as RequestInit
  expect(JSON.parse(init.body as string)).toEqual(input)
  expect(init.signal).toBe(signal)
})

it('transports opaque Filmcraft placements without parsing source clip integers', async () => {
 const { placeFilmcraftClips } = await import('./api')
 const clipText = '{"id":18446744073709551615,"start":9007199254740993,"opaque":{"nestedId":18446744073709551614}}'
 const input = { document: '{"projectId":18446744073709551615}', sequenceId: '18446744073709551614', placements: [{ trackId: '18446744073709551613', clipText }], insert: true }
 const result = { documentText: input.document, placedClipIds: ['18446744073709551615'] }
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result)))
 vi.stubGlobal('fetch', fetchMock)
 const signal = new AbortController().signal
 expect(await placeFilmcraftClips(input, signal)).toEqual(result)
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/place-clips')
 const init = fetchMock.mock.calls[0]![1] as RequestInit
 expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('copies and pastes opaque Filmcraft clipboard text with exact selection and playhead clocks', async () => {
 const { copyFilmcraftClips, pasteFilmcraftClips } = await import('./api')
 const clipboardText = '{"clips":[["Video",0,{"id":18446744073709551615,"start":9007199254740993}]],"markers":[]}'
 const document = '{"projectId":18446744073709551615}'
 const copied = { clipboardText, clipCount: 1, markerCount: 0 }
 const pasted = { documentText: document, selectedClipIds: ['18446744073709551614'], playheadTicks: '9007199254740993', playheadFrames: '849600' }
 const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(copied))).mockResolvedValueOnce(new Response(JSON.stringify(pasted)))
 vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 const copyInput = { document, sequenceId: '18446744073709551615', clipIds: ['18446744073709551614'], linked: true, includeMarkers: true }
 expect(await copyFilmcraftClips(copyInput, signal)).toEqual(copied)
 const pasteInput = { document, sequenceId: copyInput.sequenceId, clipboardText, insert: true, includeMarkers: true, rippleMarkers: true, timeFrames: '9007199254740993' }
 expect(await pasteFilmcraftClips(pasteInput, signal)).toEqual(pasted)
 for (const [i,route,input] of [[0,'copy',copyInput],[1,'paste',pasteInput]] as const) {
  expect(fetchMock.mock.calls[i]![0]).toBe(`/api/compositions/filmcraft/${route}`)
  const init = fetchMock.mock.calls[i]![1] as RequestInit; expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
 }
})

it('transports Filmcraft transition text and allocated IDs without numeric conversion', async () => {
 const { addFilmcraftTransition } = await import('./api')
 const input = { document: '{"projectId":18446744073709551615}', sequenceId: '18446744073709551614', trackId: '18446744073709551613', transitionText: '{"from":18446744073709551615,"start":9007199254740993}' }
 const result = { documentText: input.document, createdTransitionId: '9007199254740993' }
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock)
 const signal = new AbortController().signal
 expect(await addFilmcraftTransition(input, signal)).toEqual(result)
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/add-transition')
 const init = fetchMock.mock.calls[0]![1] as RequestInit
 expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('applies Filmcraft transitions with exact frame and target strings', async () => {
 const { applyFilmcraftTransition } = await import('./api')
 const input = { document: '{"projectId":18446744073709551615}', sequenceId: '18446744073709551614', kind: 'video' as const, effect: 'push', frames: '24', timeFrames: '9007199254740993', targetedTrackIds: ['18446744073709551613'], clipId: '18446744073709551612', edge: 'in' as const, params: { direction: 'From East' }, reverse: true }
 const result = { documentText: input.document, createdTransitionId: '9007199254740993' }
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 expect(await applyFilmcraftTransition(input, signal)).toEqual(result)
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/apply-transition')
 const init = fetchMock.mock.calls[0]![1] as RequestInit
 expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('loads the original Filmcraft transition catalog with cancellation', async () => {
 const { getFilmcraftTransitions } = await import('./api'); const entries = [{ definition: { id: 'push' }, instance: {} }]
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(entries))); vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 expect(await getFilmcraftTransitions(signal)).toEqual(entries); expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/transitions'); expect((fetchMock.mock.calls[0]![1] as RequestInit).signal).toBe(signal)
})

it('updates and resets an applied Filmcraft transition with an exact ID', async () => {
 const { setFilmcraftTransition } = await import('./api'); const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551614', transitionId: '18446744073709551615', reset: true, params: { direction: 'From East' }, reverse: false }
 const result = { documentText: input.document }; const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 expect(await setFilmcraftTransition(input, signal)).toEqual(result); expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/set-transition'); const init = fetchMock.mock.calls[0]![1] as RequestInit; expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('applies selection transitions with exact selection IDs and independent media durations', async () => {
 const { applyFilmcraftSelectionTransitions } = await import('./api'); const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551614', clipIds: ['18446744073709551615','9007199254740993'], videoEffect: 'cross_dissolve', audioEffect: 'constant_power', videoFrames: '30', audioFrames: '24' }
 const result = { documentText: input.document, createdTransitionIds: ['18446744073709551615','9007199254740993'] }; const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 expect(await applyFilmcraftSelectionTransitions(input, signal)).toEqual(result); expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/batch-transitions'); const init = fetchMock.mock.calls[0]![1] as RequestInit; expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('sends transition preference durations or omits duration for source defaults', async () => {
 const { applyFilmcraftTransition, applyFilmcraftSelectionTransitions } = await import('./api'); const result={documentText:'opaque',createdTransitionId:'18446744073709551615',createdTransitionIds:['18446744073709551615']};const fetchMock=vi.fn().mockImplementation(async()=>new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock)
 const single={document:'opaque',sequenceId:'18446744073709551615',kind:'audio' as const,effect:'constant_power',clipId:'9007199254740993',timeFrames:'0',duration:{value:0.5,unit:'seconds' as const}}
 await applyFilmcraftTransition(single);expect(JSON.parse((fetchMock.mock.calls[0]![1] as RequestInit).body as string)).toEqual(single)
 const batch={document:'opaque',sequenceId:'18446744073709551615',clipIds:['9007199254740993'],videoEffect:'cross_dissolve',audioEffect:'constant_power',videoDuration:{value:29.5,unit:'frames' as const},audioDuration:{value:1.25,unit:'seconds' as const}}
 await applyFilmcraftSelectionTransitions(batch);expect(JSON.parse((fetchMock.mock.calls[1]![1] as RequestInit).body as string)).toEqual(batch)
 const {duration,...defaults}=single;await applyFilmcraftTransition(defaults);const sent=JSON.parse((fetchMock.mock.calls[2]![1] as RequestInit).body as string);expect(sent).not.toHaveProperty('duration');expect(sent).not.toHaveProperty('frames')
})

it('sends ordered trim edit points without losing exact clip IDs or duplicates', async () => {
 const { applyFilmcraftSelectionTransitions } = await import('./api')
 const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551614', clipIds: [], editPoints: [{ clipId: '18446744073709551615', edge: 'out' as const }, { clipId: '9007199254740993', edge: 'in' as const }, { clipId: '18446744073709551615', edge: 'out' as const }], videoEffect: 'cross_dissolve', audioEffect: 'constant_power' }
 const result = { documentText: input.document, createdTransitionIds: ['18446744073709551615'] }
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock)
 const signal = new AbortController().signal
 expect(await applyFilmcraftSelectionTransitions(input, signal)).toEqual(result)
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/batch-transitions')
 const init = fetchMock.mock.calls[0]![1] as RequestInit
 expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('reads trim-monitor source clocks and IDs as exact strings', async () => {
 const { getFilmcraftTrimMonitor } = await import('./api')
 const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551614', clipId: '9007199254740993', edge: 'out' as const }
 const result = { editTime: '9223372036854775806', rollPair: ['9007199254740993','18446744073709551615'], outgoing: { clip: '9007199254740993', item: '18446744073709551615', time: '9007199254740993', mediaTime: '-9007199254740993', mediaFrame: '-9007199254740993', sourceIn: '-9007199254740993', sourceOut: '9223372036854775806', name: 'Клип', track: 'V1', video: true }, incoming: null }
 const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result))); vi.stubGlobal('fetch', fetchMock); const signal = new AbortController().signal
 expect(await getFilmcraftTrimMonitor(input, signal)).toEqual(result)
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/trim-monitor')
 const init=fetchMock.mock.calls[0]![1] as RequestInit; expect(JSON.parse(init.body as string)).toEqual(input); expect(init.signal).toBe(signal)
})

it('selects nearest edit points with exact clocks, tracks and clip identities', async () => {
 const { getFilmcraftNearestEditPoints } = await import('./api')
 const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551614', kind: 'roll' as const, targetedTrackIds: ['18446744073709551615'], timeTicks: '-9007199254740993' }
 const result={editPoints:[{clip:'18446744073709551615',out:true,kind:'roll'}]}
 const fetchMock=vi.fn().mockImplementation(async()=>new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock);const signal=new AbortController().signal
 expect(await getFilmcraftNearestEditPoints(input,signal)).toEqual(result);expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/nearest-edit-points');const init=fetchMock.mock.calls[0]![1] as RequestInit;expect(JSON.parse(init.body as string)).toEqual(input);expect(init.signal).toBe(signal)
 const frames={document:input.document,sequenceId:input.sequenceId,kind:'trimIn' as const,timeFrames:'9007199254740993'};await getFilmcraftNearestEditPoints(frames);expect(JSON.parse((fetchMock.mock.calls[1]![1] as RequestInit).body as string)).toEqual(frames)
})

it('returns partial trim-point edits with refusal and preserves exact IDs and deltas', async () => {
 const { trimFilmcraftEditPoints } = await import('./api')
 const input={document:'{"id":18446744073709551615}',sequenceId:'18446744073709551614',points:[{clip:'9007199254740993',out:true,kind:'ripple' as const}],linked:true,deltaTicks:'-9007199254740993'}
 const result={documentText:'{"partial":true}',appliedTicks:null,refusal:'locked later point'}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock);const signal=new AbortController().signal
 expect(await trimFilmcraftEditPoints(input,signal)).toEqual(result);expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/trim-points');const init=fetchMock.mock.calls[0]![1] as RequestInit;expect(JSON.parse(init.body as string)).toEqual(input);expect(init.signal).toBe(signal)
})

it('carries exact cumulative trim shifts through successful and refused point edits', async () => {
 const { trimFilmcraftEditPoints } = await import('./api')
 const input={document:'opaque',sequenceId:'18446744073709551615',points:[{clip:'9007199254740993',out:false,kind:'roll' as const}],linked:false,deltaFrames:'-1',shift:{outShift:'9007199254740993',inShift:'-9007199254740993'}}
 const result={documentText:'changed',appliedTicks:['-10584000000'],refusal:null,shift:{outShift:'9007188670740993',inShift:'-9007209838740993'},shiftFrames:{outShift:'851019',inShift:'-851021'}}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock)
 expect(await trimFilmcraftEditPoints(input)).toEqual(result);expect(JSON.parse((fetchMock.mock.calls[0]![1] as RequestInit).body as string)).toEqual(input)
})

it('sends exact playhead clock for extending selected trim points', async () => {
 const { trimFilmcraftEditPoints } = await import('./api')
 const input={document:'opaque',sequenceId:'1',points:[{clip:'9007199254740993',out:true,kind:'roll' as const}],linked:true,playheadTicks:'9007199254740993'}
 const result={documentText:'changed',appliedTicks:[],refusal:null,shift:{outShift:'0',inShift:'0'},shiftFrames:{outShift:'0',inShift:'0'}}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock)
 expect(await trimFilmcraftEditPoints(input)).toEqual(result);expect(JSON.parse((fetchMock.mock.calls[0]![1] as RequestInit).body as string)).toEqual(input)
})

it('transports exact Q/W selection, targets and playhead without numeric conversion', async () => {
 const { trimFilmcraftToPlayhead } = await import('./api')
 const input={document:'opaque',sequenceId:'18446744073709551615',clipIds:['9007199254740993'],targetedTrackIds:['18446744073709551614'],side:'previous' as const,ripple:true,linked:true,timeTicks:'9007199254740993'}
 const result={documentText:'changed',rangeTicks:['9007199254740992','9007199254740993'],playheadTicks:'9007199254740992',playheadFrames:'851020',trackIds:['18446744073709551614']}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(result)));vi.stubGlobal('fetch',fetchMock);const signal=new AbortController().signal
 expect(await trimFilmcraftToPlayhead(input,signal)).toEqual(result);expect(fetchMock.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/to-playhead');const init=fetchMock.mock.calls[0]![1] as RequestInit;expect(JSON.parse(init.body as string)).toEqual(input);expect(init.signal).toBe(signal)
})

it('transports authenticated trim sessions and exact revisions without conversion', async () => {
 const { createFilmcraftTrimSession,readFilmcraftTrimSession,commandFilmcraftTrimSession,closeFilmcraftTrimSession } = await import('./api')
 const fetchMock=vi.fn().mockImplementation(()=>Promise.resolve(new Response('{}')));vi.stubGlobal('fetch',fetchMock);const signal=new AbortController().signal
 const input={document:'opaque',sequenceId:'18446744073709551615',points:[{clip:'9007199254740993',out:true,kind:'roll' as const}],linked:true,shift:{outShift:'9007199254740993',inShift:'-9007199254740993'},playheadTicks:'9007199254740993'}
 await createFilmcraftTrimSession('owner-token',input,signal);await readFilmcraftTrimSession('owner-token','session',signal)
 const command={sessionId:'session',revision:'9007199254740993',action:{op:'shuttle' as const,clock:1.25,direction:'reverse' as const,slow:true}};await commandFilmcraftTrimSession('owner-token',command,signal);await closeFilmcraftTrimSession('owner-token','session','9007199254740994',signal)
 expect(fetchMock.mock.calls.map(call=>call[0])).toEqual(['create','read','command','close'].map(op=>`/api/compositions/filmcraft/trim-sessions/${op}`))
 const bodies=fetchMock.mock.calls.map(call=>JSON.parse((call[1] as RequestInit).body as string));expect(bodies).toEqual([input,{sessionId:'session'},command,{sessionId:'session',revision:'9007199254740994'}]);for(const call of fetchMock.mock.calls){const init=call[1] as RequestInit;expect(new Headers(init.headers).get('Authorization')).toBe('Bearer owner-token');expect(init.signal).toBe(signal)}
})

describe('Filmcraft program preview', () => {
  it('preserves exact project ids and clocks with cancellation and owner headers', async () => {
    const { getFilmcraftPreviewFrame } = await import('./api')
    const result = { kind: 'sequence', sequenceId: '18446744073709551615', ptsTicks: '9007199254740993', width: 1, height: 1, rgbaHex: '00000000', scale: 1, effects: true, captions: true }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(result), { headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetchMock)
    const input = { sources: { '18446744073709551614': 'image-source' }, document: '{"id":18446744073709551615}', sequenceId: result.sequenceId, timeTicks: result.ptsTicks, effects: true, captions: true }
    const controller = new AbortController()
    expect(await getFilmcraftPreviewFrame(input, controller.signal, 'owner')).toEqual(result)
    const [path, options] = fetchMock.mock.calls[0]!
    expect(path).toBe('/api/compositions/filmcraft/preview-frame')
    expect(JSON.parse(String(options.body))).toEqual(input)
    expect(options.signal).toBe(controller.signal)
    expect(options.headers).toMatchObject({ Authorization: 'Bearer owner', 'Content-Type': 'application/json' })
  })
})

describe('Filmcraft authenticated video export', () => {
  it('preserves frame strings and authenticates submission, polling, cancellation and download', async () => {
    const { startFilmcraftVideoJob, getFilmcraftVideoJob, cancelFilmcraftVideoJob, downloadFilmcraftVideo } = await import('./api')
    const fetchMock = vi.fn().mockImplementation(async (path: string) => path.startsWith('/files/') ? new Response('mkv', { headers: { 'content-type': 'video/x-matroska' } }) : new Response(JSON.stringify({ jobId: 'job', status: 'running' }), { headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetchMock)
    const controller = new AbortController()
    const input = { document: '{"id":18446744073709551615}', sequenceId: '18446744073709551615', startFrames: '9007199254740993', frames: 48, withAudio: true, sources: { '18446744073709551614': 'source' } }
    await startFilmcraftVideoJob('owner', input, controller.signal)
    await getFilmcraftVideoJob('owner', 'job/id', controller.signal)
    await cancelFilmcraftVideoJob('owner', 'job/id', controller.signal)
    const blob = await downloadFilmcraftVideo('owner', '12345678-1234-1234-1234-123456789abc', controller.signal)
    expect(await blob.text()).toBe('mkv')
    expect(fetchMock.mock.calls.map(call => call[0])).toEqual(['/api/compositions/filmcraft/video-jobs', '/api/jobs/job%2Fid', '/api/jobs/job%2Fid/cancel', '/files/outputs/12345678-1234-1234-1234-123456789abc.mkv'])
    expect(JSON.parse(String(fetchMock.mock.calls[0][1].body))).toEqual(input)
    for (const [, options] of fetchMock.mock.calls) { expect(options.headers).toMatchObject({ Authorization: 'Bearer owner' }); expect(options.signal).toBe(controller.signal) }
    await expect(downloadFilmcraftVideo('owner', '../foreign')).rejects.toThrow('идентификатор')
    expect(fetchMock).toHaveBeenCalledTimes(4)
  })
})

it('loads Filmcraft export history with an encoded owner cursor and cancellation', async () => {
  const { listFilmcraftVideoJobs } = await import('./api')
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ jobs: [], nextCursor: null }), { headers: { 'content-type': 'application/json' } }))
  vi.stubGlobal('fetch', fetchMock)
  const controller = new AbortController()
  expect(await listFilmcraftVideoJobs('owner', 'page/id', controller.signal)).toEqual({ jobs: [], nextCursor: null })
  expect(fetchMock).toHaveBeenCalledWith('/api/compositions/filmcraft/video-jobs?limit=20&cursor=page%2Fid', expect.objectContaining({ headers: { Authorization: 'Bearer owner' }, signal: controller.signal }))
})

it('retries a Filmcraft export with owner headers, escaped job id and cancellation', async () => {
  const { retryFilmcraftVideoJob } = await import('./api')
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ jobId: 'job/id', status: 'pending' }), { headers: { 'content-type': 'application/json' } }))
  vi.stubGlobal('fetch', fetchMock)
  const controller = new AbortController()
  expect(await retryFilmcraftVideoJob('owner', 'job/id', controller.signal)).toEqual({ jobId: 'job/id', status: 'pending' })
  expect(fetchMock).toHaveBeenCalledWith('/api/jobs/job%2Fid/retry', expect.objectContaining({ method: 'POST', headers: { Authorization: 'Bearer owner' }, signal: controller.signal }))
})

it('submits a Filmcraft H264 preset and authenticates MP4 artifact download', async () => {
  const { startFilmcraftVideoJob, downloadFilmcraftVideo } = await import('./api')
  const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'job', mode: 'video_audio', container: 'mp4' }), { headers: { 'content-type': 'application/json' } })).mockResolvedValueOnce(new Response('mp4'))
  vi.stubGlobal('fetch', fetchMock)
  const input = { document: '{}', sequenceId: '6', startFrames: '0', frames: 4, sources: {}, withAudio: true, preset: 'adaptive_medium' as const, frameSize: [80, 80] as [number, number], scaling: 'scaleToFill' as const, pixelAspect: [2, 1] as [number, number], maxRenderQuality: true, metadata: { title: ' Кадр 🎬 ', creator: 'Автор', copyright: '© 2026', description: 'first\nsecond', comment: 'literal = -metadata' }, videoLimiter: { enabled: true, minPercent: 10, maxPercent: 80.5 } }
  await startFilmcraftVideoJob('owner', input)
  expect(JSON.parse(String(fetchMock.mock.calls[0][1].body))).toEqual(input)
  await downloadFilmcraftVideo('owner', '12345678-1234-1234-1234-123456789abc', undefined, 'mp4')
  expect(fetchMock.mock.calls[1][0]).toBe('/files/outputs/12345678-1234-1234-1234-123456789abc.mp4')
  expect(fetchMock.mock.calls[1][1].headers).toMatchObject({ Authorization: 'Bearer owner' })
})

it('submits ProRes and downloads its MOV through authenticated transport', async () => {
  const { startFilmcraftVideoJob, downloadFilmcraftVideo } = await import('./api')
  const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ jobId: 'mov', mode: 'video_audio', container: 'mov' }), { headers: { 'content-type': 'application/json' } })).mockResolvedValueOnce(new Response('mov'))
  vi.stubGlobal('fetch', fetchMock)
  const controller = new AbortController()
  const input = { document: '{}', sequenceId: '6', startFrames: '0', frames: 4, sources: {}, preset: 'prores_hq' as const, withAudio: true }
  await startFilmcraftVideoJob('owner', input, controller.signal)
  expect(JSON.parse(String(fetchMock.mock.calls[0][1].body))).toEqual(input)
  await downloadFilmcraftVideo('owner', '12345678-1234-1234-1234-123456789abc', controller.signal, 'mov')
  expect(fetchMock.mock.calls[1][0]).toBe('/files/outputs/12345678-1234-1234-1234-123456789abc.mov')
  expect(fetchMock.mock.calls[1][1]).toMatchObject({ headers: { Authorization: 'Bearer owner' }, signal: controller.signal })
})
