import * as api from '$lib/api.js'
import type { Job } from '$lib/types.js'
import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import {
  PROXY_PREFERENCE_STORAGE_KEY,
  parseProxyPreferences,
  resolveProxyPlayback,
  serializeProxyPreferences,
  type ProxyPlaybackSelection,
  type ProxyPreferences,
} from './model.js'
import { requireProxyKey, type ProxyCreateResult, type ProxyList, type ProxyProfile } from './types.js'

export interface ProxyClient {
  createLibraryProxy(sourceId: string, profile: ProxyProfile): Promise<ProxyCreateResult>
  getLibraryProxies(sourceId: string): Promise<ProxyList>
  deleteLibraryProxy(sourceId: string, key: string): Promise<void>
  pollJob(jobId: string, onTick?: (job: Job) => void): Promise<Job>
}

export interface ProxySourceUiState {
  phase: 'idle' | 'loading' | 'ready' | 'error'
  list: ProxyList | null
  error: string
  playbackError: string
  failedKey: string | null
  submitting: boolean
}

export interface ProxyControllerState {
  sources: Record<string, ProxySourceUiState>
  preferences: ProxyPreferences
}

interface StorageLike {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

function browserStorage(): StorageLike | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage
  } catch {
    return null
  }
}

function initialPreferences(storage: StorageLike | null): ProxyPreferences {
  if (!storage) return {}
  try {
    return parseProxyPreferences(storage.getItem(PROXY_PREFERENCE_STORAGE_KEY))
  } catch {
    return {}
  }
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function isCancelled(error: unknown): boolean {
  return error instanceof Error && error.message === 'cancelled'
}

export function createProxyController(
  client: ProxyClient = api,
  storage: StorageLike | null = browserStorage(),
) {
  const state = $state<ProxyControllerState>({
    sources: {},
    preferences: initialPreferences(storage),
  })
  const refreshRevisions = new SvelteMap<string, number>()
  const monitoredJobs = new SvelteSet<string>()
  const ignoredKeys = new SvelteMap<string, SvelteSet<string>>()

  function source(sourceId: string): ProxySourceUiState {
    let current = state.sources[sourceId]
    if (!current) {
      state.sources[sourceId] = {
        phase: 'idle',
        list: null,
        error: '',
        playbackError: '',
        failedKey: null,
        submitting: false,
      }
      current = state.sources[sourceId]
    }
    return current
  }

  function persistPreferences(): void {
    if (!storage) return
    try {
      if (Object.keys(state.preferences).length === 0) storage.removeItem(PROXY_PREFERENCE_STORAGE_KEY)
      else storage.setItem(PROXY_PREFERENCE_STORAGE_KEY, serializeProxyPreferences(state.preferences))
    } catch {
      // Private browsing/storage quotas must not break local playback.
    }
  }

  function setOriginal(sourceId: string): void {
    state.preferences[sourceId] = { mode: 'original' }
    const current = source(sourceId)
    current.failedKey = null
    current.playbackError = ''
    persistPreferences()
  }

  function setProxy(sourceId: string, key: string): void {
    state.preferences[sourceId] = { mode: 'proxy', key: requireProxyKey(key) }
    const current = source(sourceId)
    current.failedKey = null
    current.playbackError = ''
    persistPreferences()
  }

  function patchJob(sourceId: string, jobId: string, job: Job): void {
    const current = source(sourceId)
    const existing = current.list?.jobs.find((candidate) => candidate.jobId === jobId)
    if (!existing || ignoredKeys.get(sourceId)?.has(existing.key)) return
    if (job.status !== 'pending' && job.status !== 'running') return
    existing.status = job.status
    existing.progress = typeof job.progress === 'number' ? job.progress : undefined
    existing.stage = job.stage
  }

  function monitor(sourceId: string, jobId: string): void {
    if (monitoredJobs.has(jobId)) return
    monitoredJobs.add(jobId)
    void client.pollJob(jobId, (job) => patchJob(sourceId, jobId, job)).then(
      () => refresh(sourceId),
      async (error) => {
        const terminalError = isCancelled(error) ? '' : message(error)
        const refreshed = await refresh(sourceId)
        if (terminalError && refreshed) source(sourceId).error = terminalError
      },
    ).finally(() => monitoredJobs.delete(jobId))
  }

  async function refresh(sourceId: string): Promise<ProxyList | null> {
    const current = source(sourceId)
    const revision = (refreshRevisions.get(sourceId) ?? 0) + 1
    refreshRevisions.set(sourceId, revision)
    if (!current.list) current.phase = 'loading'
    current.error = ''
    try {
      const list = await client.getLibraryProxies(sourceId)
      if (refreshRevisions.get(sourceId) !== revision) return current.list
      current.list = list
      current.phase = 'ready'
      for (const job of list.jobs) monitor(sourceId, job.jobId)
      return list
    } catch (error) {
      if (refreshRevisions.get(sourceId) !== revision) return current.list
      current.error = message(error)
      current.phase = 'error'
      return null
    }
  }

  function ensure(sourceId: string): void {
    if (!state.sources[sourceId] || state.sources[sourceId]?.phase === 'idle') void refresh(sourceId)
  }

  async function generate(sourceId: string, profile: ProxyProfile): Promise<ProxyCreateResult | null> {
    const current = source(sourceId)
    if (current.submitting) return null
    current.submitting = true
    current.error = ''
    try {
      const created = await client.createLibraryProxy(sourceId, profile)
      ignoredKeys.get(sourceId)?.delete(created.key)
      setProxy(sourceId, created.key)
      await refresh(sourceId)
      monitor(sourceId, created.jobId)
      return created
    } catch (error) {
      current.error = message(error)
      current.phase = 'error'
      return null
    } finally {
      current.submitting = false
    }
  }

  async function remove(sourceId: string, key: string): Promise<boolean> {
    const current = source(sourceId)
    if (current.submitting) return false
    current.submitting = true
    current.error = ''
    const ignored = ignoredKeys.get(sourceId) ?? new SvelteSet<string>()
    ignored.add(key)
    ignoredKeys.set(sourceId, ignored)
    try {
      await client.deleteLibraryProxy(sourceId, key)
      if (state.preferences[sourceId]?.mode === 'proxy' && state.preferences[sourceId]?.key === key) {
        setOriginal(sourceId)
      }
      if (current.failedKey === key) current.failedKey = null
      await refresh(sourceId)
      return true
    } catch (error) {
      ignored.delete(key)
      current.error = message(error)
      current.phase = 'error'
      return false
    } finally {
      current.submitting = false
    }
  }

  function playback(sourceId: string, originalUrl: string): ProxyPlaybackSelection {
    const current = state.sources[sourceId]
    return resolveProxyPlayback(
      originalUrl,
      current?.list ?? null,
      state.preferences[sourceId],
      current?.failedKey ?? null,
    )
  }

  function markPlaybackFailed(sourceId: string, key: string): void {
    const current = source(sourceId)
    current.failedKey = key
    current.playbackError = 'Proxy не удалось воспроизвести — используется оригинал.'
  }

  function forget(sourceId: string): void {
    delete state.sources[sourceId]
    delete state.preferences[sourceId]
    refreshRevisions.delete(sourceId)
    ignoredKeys.delete(sourceId)
    persistPreferences()
  }

  function resetForTests(): void {
    for (const key of Object.keys(state.sources)) delete state.sources[key]
    for (const key of Object.keys(state.preferences)) delete state.preferences[key]
    refreshRevisions.clear()
    monitoredJobs.clear()
    ignoredKeys.clear()
    persistPreferences()
  }

  return {
    state,
    ensure,
    refresh,
    generate,
    remove,
    setOriginal,
    setProxy,
    playback,
    markPlaybackFailed,
    forget,
    resetForTests,
  }
}

export type ProxyController = ReturnType<typeof createProxyController>
export const proxyController = createProxyController()
