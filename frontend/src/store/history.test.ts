// Covers the seam between store.ts and the feature modules: undo/redo, project
// autosave/restore and "open another clip" must all carry module state.

import { nextTick } from 'vue'
import { describe, it, expect, beforeEach, vi } from 'vitest'
import * as api from '../api'
import {
  beginEditTransaction,
  buildEditPayload,
  endEditTransaction,
  hasMeaningfulChanges,
  openFromLibrary,
  redo,
  resetHistory,
  state,
  undo,
} from '../store'
import { defaultEdit } from '../domain/edit'
import type { MediaEntry, VideoInfo } from '../types'
import { colorAdvancedState } from './colorAdvanced'
import { motionState } from './motion'
import { featureModulesActive, resetFeatureModules } from './modules'

vi.mock('../api', () => {
  class ApiError extends Error {
    constructor(
      message: string,
      readonly status: number,
      readonly code?: string,
    ) {
      super(message)
    }
  }
  class BackendUnavailableError extends Error {}
  class AssetsRequireServerError extends Error {}

  return {
    clientOnlyMode: false,
    ApiError,
    BackendUnavailableError,
    AssetsRequireServerError,
    importUrl: vi.fn(),
    uploadFile: vi.fn(),
    uploadLut: vi.fn(),
    getLut: vi.fn(),
    edit: vi.fn(),
    pollJob: vi.fn(),
    getLibrary: vi.fn(() => Promise.resolve([])),
    deleteLibraryItem: vi.fn(),
    getAssets: vi.fn(() => Promise.resolve([])),
    uploadAsset: vi.fn(),
    deleteAsset: vi.fn(),
    saveProject: vi.fn(() => Promise.resolve({})),
    getProjectByVideo: vi.fn(() => Promise.resolve(null)),
    getProjects: vi.fn(() => Promise.resolve([])),
    deleteProject: vi.fn(),
    cancelJob: vi.fn(),
    getCapabilities: vi.fn(() => Promise.resolve(null)),
  }
})

vi.mock('../toasts', () => ({ toast: vi.fn() }))

const ENTRY: MediaEntry = {
  id: 'vid',
  kind: 'source',
  filename: 'vid.mp4',
  url: '/files/sources/vid.mp4',
  duration: 10,
  width: 1280,
  height: 720,
  createdAt: 0,
}

const VIDEO: VideoInfo = {
  id: ENTRY.id,
  url: ENTRY.url,
  filename: ENTRY.filename,
  duration: 10,
  width: 1280,
  height: 720,
  title: null,
  sizeBytes: null,
}

beforeEach(async () => {
  vi.mocked(api.getProjectByVideo).mockReset().mockResolvedValue(null)
  vi.mocked(api.saveProject).mockClear()
  resetFeatureModules()
  state.video = { ...VIDEO }
  const edit = defaultEdit()
  edit.trimEnd = 10
  edit.crop = { x: 0, y: 0, w: 1280, h: 720 }
  edit.scale = { w: 1280, h: -2 }
  state.edit = edit
  await nextTick()
  resetHistory()
})

describe('undo/redo across feature modules', () => {
  it('restores module state, not just the edit fields', async () => {
    beginEditTransaction('color-wheels')
    colorAdvancedState.exposure = 0.75
    endEditTransaction()
    await nextTick()

    expect(buildEditPayload().colorAdvanced).toEqual({ exposure: 0.75 })

    undo()
    expect(colorAdvancedState.exposure).toBe(0)
    expect('colorAdvanced' in buildEditPayload()).toBe(false)

    redo()
    expect(colorAdvancedState.exposure).toBe(0.75)
    expect(buildEditPayload().colorAdvanced).toEqual({ exposure: 0.75 })
  })

  it('keeps an edit-field change and a module change in one transaction', async () => {
    beginEditTransaction('mixed')
    state.edit.rotate = 90
    motionState.speedRamps = [{ t: 0, v: 2, interp: 'linear' }]
    endEditTransaction()
    await nextTick()

    undo()
    expect(state.edit.rotate).toBe(0)
    expect(motionState.speedRamps).toEqual([])

    redo()
    expect(state.edit.rotate).toBe(90)
    expect(motionState.speedRamps).toEqual([{ t: 0, v: 2, interp: 'linear' }])
  })
})

describe('export payload seam', () => {
  it('sends exactly today\'s payload while every module is at defaults', () => {
    expect(buildEditPayload()).toEqual({ videoId: 'vid', mute: false, speed: 1 })
    expect(hasMeaningfulChanges()).toBe(false)
  })

  it('reports meaningful changes as soon as a module contributes', () => {
    colorAdvancedState.tint = 0.2
    expect(hasMeaningfulChanges()).toBe(true)
  })
})

describe('project autosave and restore', () => {
  it('omits the modules block for a project that uses no new feature', async () => {
    state.edit.rotate = 180
    // The project autosave is debounced by a second.
    await vi.waitFor(() => expect(api.saveProject).toHaveBeenCalled(), { timeout: 4000 })

    const body = vi.mocked(api.saveProject).mock.calls.at(-1)?.[0] as Record<string, unknown>
    const edit = body.edit as Record<string, unknown>
    expect('modules' in edit).toBe(false)
  })

  it('stores the modules block once a module is active', async () => {
    colorAdvancedState.exposure = -1
    // The project autosave is debounced by a second.
    await vi.waitFor(() => expect(api.saveProject).toHaveBeenCalled(), { timeout: 4000 })

    const body = vi.mocked(api.saveProject).mock.calls.at(-1)?.[0] as Record<string, unknown>
    const edit = body.edit as Record<string, unknown>
    const modules = edit.modules as Record<string, Record<string, unknown>>
    expect(modules.colorAdvanced.exposure).toBe(-1)
  })

  it('restores module state when a saved clip is reopened', async () => {
    vi.mocked(api.getProjectByVideo).mockResolvedValue({
      id: 'p1',
      name: 'vid',
      videoId: 'vid',
      video: { ...VIDEO },
      edit: {
        rotate: 90,
        modules: { colorAdvanced: { exposure: 1.5 }, spatial: { stabilize: { mode: 'fast' } } },
      },
      createdAt: 0,
      updatedAt: 0,
    })

    openFromLibrary(ENTRY)
    await vi.waitFor(() => expect(state.edit.rotate).toBe(90))

    expect(colorAdvancedState.exposure).toBe(1.5)
    expect(buildEditPayload().stabilize).toEqual({
      mode: 'fast',
      smoothing: 10,
      zoom: 0,
      horizonLock: false,
    })
    // The persisted `modules` key must never leak into EditState.
    expect('modules' in state.edit).toBe(false)
  })

  it('resets module state when a clip without a saved project is opened', async () => {
    colorAdvancedState.exposure = 2

    openFromLibrary(ENTRY)
    await nextTick()

    expect(featureModulesActive()).toBe(false)
  })
})
