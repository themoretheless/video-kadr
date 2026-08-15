// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  addMediaInfoToComposition,
  compositionState,
  resetCompositionForTests,
  setCompositionPlayhead,
} from '$lib/state/composition.svelte.js'
import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'
import {
  compositionTemplateState,
  resetCompositionTemplateCatalogForTests,
} from '$lib/state/compositionTemplates.svelte.js'
import { state as legacyState } from '$lib/state/store.svelte.js'
import type { CaptureRuntime, RecorderPort } from '$lib/capture/types.js'
import type { AudioGraphPort, AudioRecorderPort, VoiceoverRuntime } from '$lib/audio/types.js'
import type { MediaInfo } from '$lib/types.js'
import CompositionWorkspace from './CompositionWorkspace.svelte'

let target: HTMLDivElement

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('[]', { status: 200 })))
  resetCompositionForTests()
  resetCompositionTemplateCatalogForTests()
  legacyState.library = []
  legacyState.librarySnapshotReady = false
  legacyState.importError = ''
  legacyState.importing = false
  addMediaInfoToComposition({
    id: 'workspace-video',
    url: '/files/sources/workspace.mp4',
    filename: 'workspace.mp4',
    mediaType: 'video',
    duration: 5,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  target?.remove()
})

async function settle(): Promise<void> {
  await tick()
  await Promise.resolve()
  await tick()
}

function button(text: string): HTMLButtonElement {
  const result = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text))
  if (!result) throw new Error(`Missing button: ${text}`)
  return result
}

class WorkspaceVoiceoverTrack {
  stopped = false
  stop(): void { this.stopped = true }
  getSettings(): MediaTrackSettings { return {} }
  addEventListener(): void {}
  removeEventListener(): void {}
}

class WorkspaceVoiceoverRecorder implements AudioRecorderPort {
  readonly mimeType = 'audio/webm'
  state: RecordingState = 'inactive'
  ondataavailable: ((event: BlobEvent) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onstop: ((event: Event) => void) | null = null
  start(): void { this.state = 'recording' }
  pause(): void { this.state = 'paused' }
  resume(): void { this.state = 'recording' }
  stop(): void {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['voiceover'], { type: 'audio/webm' }) } as BlobEvent)
    this.onstop?.(new Event('stop'))
  }
}

function voiceoverRuntime(): VoiceoverRuntime {
  const track = new WorkspaceVoiceoverTrack()
  const stream = {
    getAudioTracks: () => [track],
    getTracks: () => [track],
  } as unknown as MediaStream
  const graph: AudioGraphPort = {
    output: stream,
    readLevel: () => ({ rms: 0.1, peak: 0.2 }),
    cleanup: vi.fn(),
  }
  return {
    isSupported: () => true,
    getUserMedia: vi.fn(async () => stream),
    enumerateDevices: vi.fn(async () => []),
    createGraph: vi.fn(async () => graph),
    createRecorder: vi.fn(() => new WorkspaceVoiceoverRecorder()),
    isMimeTypeSupported: () => true,
    createObjectURL: vi.fn(() => 'blob:workspace-voiceover'),
    revokeObjectURL: vi.fn(),
    now: () => Date.now(),
    setTimeout: (callback, delay) => setTimeout(callback, delay),
    clearTimeout: (timer) => clearTimeout(timer),
    setInterval: (callback, delay) => setInterval(callback, delay),
    clearInterval: (timer) => clearInterval(timer),
  }
}

describe('CompositionWorkspace local integrations', () => {
  it('guards the New control from discarding an active unsaved composition', async () => {
    const before = JSON.stringify(compositionState.document)
    const component = mount(CompositionWorkspace, { target })
    await settle()

    button('Новый').click()
    await settle()

    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(target.querySelector('[role="alert"]')?.textContent).toContain('Сначала сохраните текущую композицию')
    await unmount(component)
  })

  it('exposes CapturePanel and imports SRT plus a deterministic template through reachable controls', async () => {
    const component = mount(CompositionWorkspace, { target })
    await settle()

    const capture = [...target.querySelectorAll('details')].find((details) => details.textContent?.includes('Записать экран'))!
    capture.open = true
    capture.dispatchEvent(new Event('toggle'))
    await settle()
    expect(target.textContent).toContain('Запись для композиции')

    const srtInput = target.querySelector<HTMLInputElement>('input[accept*=".srt"]')!
    const srt = new File(
      ['1\n00:00:00,000 --> 00:00:01,000\nЛокальный субтитр'],
      'captions.srt',
      { type: 'application/x-subrip' },
    )
    Object.defineProperty(srtInput, 'files', { configurable: true, value: [srt] })
    srtInput.dispatchEvent(new Event('change', { bubbles: true }))
    await settle()
    expect(compositionState.document.tracks.find((track) => track.kind === 'text')?.clips[0]).toMatchObject({
      text: 'Локальный субтитр',
    })

    button('Создать из композиции').click()
    await settle()
    expect(compositionTemplateState.templates).toHaveLength(1)
    expect(compositionTemplateState.templates[0]!.slots).toHaveLength(2)

    await unmount(component)
  })

  it('uploads a completed CapturePanel file and appends it without opening legacy state', async () => {
    const display = {
      getTracks: () => [],
      getVideoTracks: () => [],
      getAudioTracks: () => [],
    } as unknown as MediaStream
    let recorderState: RecordingState = 'inactive'
    const recorder: RecorderPort = {
      mimeType: 'video/webm',
      get state() { return recorderState },
      ondataavailable: null,
      onerror: null,
      onstop: null,
      start() { recorderState = 'recording' },
      pause() { recorderState = 'paused' },
      resume() { recorderState = 'recording' },
      stop() {
        recorderState = 'inactive'
        recorder.ondataavailable?.({ data: new Blob(['capture'], { type: 'video/webm' }) } as BlobEvent)
        recorder.onstop?.(new Event('stop'))
      },
    }
    const runtime: CaptureRuntime = {
      isSupported: () => true,
      getDisplayMedia: vi.fn(async () => display),
      getUserMedia: vi.fn(async () => display),
      enumerateDevices: vi.fn(async () => []),
      prepareStream: vi.fn(async () => ({ stream: display, previewCanvas: document.createElement('canvas'), cleanup: vi.fn() })),
      createRecorder: vi.fn(() => recorder),
      isMimeTypeSupported: () => true,
      createObjectURL: () => 'blob:capture-preview',
      revokeObjectURL: vi.fn(),
      now: () => Date.now(),
      setTimeout: (callback, delay) => setTimeout(callback, delay),
      clearTimeout: (timer) => clearTimeout(timer),
      setInterval: (callback, delay) => setInterval(callback, delay),
      clearInterval: (timer) => clearInterval(timer),
    }
    const uploader = vi.fn(async () => ({
      id: 'captured-video',
      url: '/files/sources/captured.webm',
      filename: 'captured.webm',
      mediaType: 'video' as const,
      duration: 3,
      width: 1280,
      height: 720,
      acodec: 'opus',
    }))
    const component = mount(CompositionWorkspace, {
      target,
      props: { captureRuntime: runtime, captureCountdownSeconds: 0, captureUploader: uploader },
    })
    await settle()

    const capture = [...target.querySelectorAll('details')].find((details) => details.textContent?.includes('Записать экран'))!
    capture.open = true
    capture.dispatchEvent(new Event('toggle'))
    await settle()
    button('Начать запись').click()
    await settle()
    expect(target.textContent).toContain('Идёт запись')
    button('Завершить').click()
    await settle()
    await settle()

    expect(uploader).toHaveBeenCalledWith(expect.any(File), false)
    expect(compositionState.document.sources).toHaveProperty('captured-video')
    expect(compositionState.document.tracks.find((track) => track.kind === 'video')?.clips).toHaveLength(2)

    await unmount(component)
  })

  it('uploads a completed voiceover with busy UX and inserts an audio clip at the playhead', async () => {
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    let resolveUpload!: (media: MediaInfo | null) => void
    const uploader = vi.fn(() => new Promise<MediaInfo | null>((resolve) => { resolveUpload = resolve }))
    const component = mount(CompositionWorkspace, {
      target,
      props: {
        voiceoverRuntime: voiceoverRuntime(),
        voiceoverCountdownSeconds: 0,
        voiceoverUploader: uploader,
      },
    })
    await settle()

    const voiceover = [...target.querySelectorAll('details')].find((details) => details.textContent?.includes('Записать голос'))!
    voiceover.open = true
    voiceover.dispatchEvent(new Event('toggle'))
    await settle()
    expect(target.textContent).toContain('Voiceover для композиции')

    button('Начать запись').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Идёт запись голоса'))
    button('Завершить').click()
    await vi.waitFor(() => expect(uploader).toHaveBeenCalledWith(expect.any(File), false))
    await settle()
    expect(target.textContent).toContain('Загружаю голосовую запись')
    expect(button('Сохраняем…').disabled).toBe(true)

    resolveUpload({
      id: 'workspace-voiceover',
      url: '/files/sources/workspace-voiceover.webm',
      filename: 'workspace-voiceover.webm',
      mediaType: 'audio',
      duration: 4,
      width: 0,
      height: 0,
      acodec: 'opus',
    })
    await vi.waitFor(() => expect(compositionState.document.sources).toHaveProperty('workspace-voiceover'))
    await settle()

    const audioTrack = compositionState.document.tracks.find((track) => track.kind === 'audio')!
    expect(audioTrack.clips[0]).toMatchObject({
      kind: 'audio',
      sourceId: 'workspace-voiceover',
      timelineStartTicks: 2 * COMPOSITION_TIME_BASE,
      sourceOutTicks: 3 * COMPOSITION_TIME_BASE,
    })
    expect(target.textContent).toContain('Голосовая запись добавлена')
    expect([...target.querySelectorAll('details')].some((details) => details.textContent?.includes('Записать экран'))).toBe(true)

    await unmount(component)
  })

  it('keeps the recorded voiceover available and reports an upload failure', async () => {
    legacyState.importError = 'Хранилище недоступно'
    const uploader = vi.fn(async () => null)
    const component = mount(CompositionWorkspace, {
      target,
      props: {
        voiceoverRuntime: voiceoverRuntime(),
        voiceoverCountdownSeconds: 0,
        voiceoverUploader: uploader,
      },
    })
    await settle()

    const voiceover = [...target.querySelectorAll('details')].find((details) => details.textContent?.includes('Записать голос'))!
    voiceover.open = true
    voiceover.dispatchEvent(new Event('toggle'))
    await settle()
    button('Начать запись').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Идёт запись голоса'))
    button('Завершить').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Хранилище недоступно'))

    expect(compositionState.document.tracks.some((track) => track.kind === 'audio')).toBe(false)
    expect(target.querySelector('audio[aria-label="Предпросмотр голосовой записи"]')).not.toBeNull()
    expect(button('Повторить загрузку').disabled).toBe(false)
    expect(button('Скачать запись')).toBeTruthy()
    expect(button('Сначала сохраните текущую запись').disabled).toBe(true)
    await unmount(component)
  })

  it('retains a failed voiceover across closing and retries the same downloadable File', async () => {
    legacyState.importError = 'Временная ошибка хранилища'
    const uploaded: MediaInfo = {
      id: 'retried-voiceover',
      url: '/files/sources/retried-voiceover.webm',
      filename: 'retried-voiceover.webm',
      mediaType: 'audio',
      duration: 2,
      width: 0,
      height: 0,
      acodec: 'opus',
    }
    const uploader = vi.fn()
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(uploaded)
    const createObjectURL = vi.fn(() => 'blob:pending-voiceover')
    const revokeObjectURL = vi.fn()
    vi.stubGlobal('URL', { createObjectURL, revokeObjectURL })
    const downloads: Array<{ href: string; filename: string }> = []
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      downloads.push({ href: this.href, filename: this.download })
    })
    const component = mount(CompositionWorkspace, {
      target,
      props: {
        voiceoverRuntime: voiceoverRuntime(),
        voiceoverCountdownSeconds: 0,
        voiceoverUploader: uploader,
      },
    })
    await settle()

    const voiceover = [...target.querySelectorAll('details')].find((details) => details.textContent?.includes('Записать голос'))!
    voiceover.open = true
    voiceover.dispatchEvent(new Event('toggle'))
    await settle()
    button('Начать запись').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Идёт запись голоса'))
    button('Завершить').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Временная ошибка хранилища'))

    const pendingFile = uploader.mock.calls[0]?.[0] as File
    expect(pendingFile).toBeInstanceOf(File)
    expect(target.textContent).toContain(pendingFile.name)
    voiceover.open = false
    voiceover.dispatchEvent(new Event('toggle'))
    await settle()
    expect(button('Повторить загрузку').disabled).toBe(false)

    button('Скачать запись').click()
    expect(createObjectURL).toHaveBeenCalledWith(pendingFile)
    expect(downloads).toEqual([{ href: 'blob:pending-voiceover', filename: pendingFile.name }])

    voiceover.open = true
    voiceover.dispatchEvent(new Event('toggle'))
    await settle()
    expect(button('Сначала сохраните текущую запись').disabled).toBe(true)
    expect(uploader).toHaveBeenCalledTimes(1)

    button('Повторить загрузку').click()
    await vi.waitFor(() => expect(compositionState.document.sources).toHaveProperty(uploaded.id))
    expect(uploader).toHaveBeenNthCalledWith(2, pendingFile, false)
    expect(target.querySelector('[aria-label="Несохранённая голосовая запись"]')).toBeNull()
    expect(compositionState.document.tracks.find((track) => track.kind === 'audio')?.clips[0]).toMatchObject({
      sourceId: uploaded.id,
    })

    await unmount(component)
  })

  it('shows missing-media recovery only after an authoritative library snapshot', async () => {
    const sourceBefore = JSON.parse(JSON.stringify(compositionState.document.sources['workspace-video']!)) as unknown
    const component = mount(CompositionWorkspace, { target })
    await settle()

    expect(compositionState.media).toHaveProperty('workspace-video')
    expect(target.textContent).not.toContain('Не найдено медиафайлов')

    legacyState.librarySnapshotReady = true
    await settle()

    expect(compositionState.media).not.toHaveProperty('workspace-video')
    expect(compositionState.document.sources['workspace-video']).toEqual(sourceBefore)
    expect(compositionState.document.tracks[0]?.clips).toHaveLength(1)
    expect(target.textContent).toContain('Не найдено медиафайлов: 1')
    expect(target.textContent).toContain('Восстановить медиа')
    expect(target.querySelector('select[aria-label="Замена для workspace-video"]')).not.toBeNull()

    await unmount(component)
  })

  it('imports a portable project, refreshes its media and opens the relinked document', async () => {
    const current = compositionState.document
    const importedDocument = {
      ...current,
      sources: {
        'relinked-source': {
          ...current.sources['workspace-video']!,
          id: 'relinked-source',
        },
      },
      tracks: current.tracks.map((track) => ({
        ...track,
        clips: track.clips.map((clip) => 'sourceId' in clip ? { ...clip, sourceId: 'relinked-source' } : clip),
      })),
    }
    const project = {
      id: 'portable-project',
      name: 'Portable cut',
      schemaVersion: 2,
      mode: 'composition',
      document: importedDocument,
      sourceIds: ['relinked-source'],
      createdAt: 2,
      updatedAt: 2,
    }
    const library = [{
      id: 'relinked-source',
      kind: 'source',
      filename: 'relinked.mp4',
      url: '/files/sources/relinked.mp4',
      mediaType: 'video',
      duration: 5,
      width: 1280,
      height: 720,
      createdAt: 2,
    }]
    const fetchMock = vi.fn(async (path: RequestInfo | URL) => {
      if (path === '/api/composition-projects/import') {
        return new Response(JSON.stringify({ project, sourceMapping: { 'workspace-video': 'relinked-source' } }), { status: 201 })
      }
      if (path === '/api/library') return new Response(JSON.stringify(library), { status: 200 })
      if (path === '/api/composition-projects') return new Response(JSON.stringify([project]), { status: 200 })
      if (path === '/api/composition-projects/portable-project') return new Response(JSON.stringify(project), { status: 200 })
      return new Response('not found', { status: 404 })
    })
    vi.stubGlobal('fetch', fetchMock)
    const component = mount(CompositionWorkspace, { target })
    await settle()

    const input = target.querySelector<HTMLInputElement>('input[accept*=".veproj"]')!
    const file = new File(['VEPROJ\r\n'], 'portable.veproj', { type: 'application/vnd.video-editor.project' })
    Object.defineProperty(input, 'files', { configurable: true, value: [file] })
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(compositionState.projectId).toBe('portable-project'))
    await settle()

    expect(compositionState.document.sources).toHaveProperty('relinked-source')
    expect(compositionState.media['relinked-source']).toMatchObject({ url: '/files/sources/relinked.mp4' })
    expect(target.textContent).toContain('Импортирован проект «Portable cut» · 1 медиа')

    await unmount(component)
  })
})
