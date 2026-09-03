import { afterEach, describe, expect, it, vi } from 'vitest'

import {
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
} from './api'
import { addClip, addTrack, createComposition, registerSource } from './composition/commands'
import { buildCompositionRenderRequest } from './composition/payload'
import { createCompositionTemplate } from './composition/templates'

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
