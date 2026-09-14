import * as api from '../../api.js'
import {
  fetchLibrary,
  updateCachedLibrary
} from '../../../data/library.js'
import {
  toast
} from '../toasts.svelte.js'
import {
  authState
} from '../auth.svelte.js'
import {
  spacesState
} from '../spaces.svelte.js'
import {
  defaultEdit
} from '../../domain/edit.js'
import {
  resetHistory
} from './history.svelte.js'
import {
  state,
  restoreProject,
  markRestoringProject
} from './core.svelte.js'
import type {
  MediaEntry,
  VideoInfo
} from '../../types'

let libraryLoadRevision = 0

export async function loadLibrary(): Promise<boolean> {
  const revision = ++libraryLoadRevision
  state.librarySnapshotReady = false
  try {
    const entries = await fetchLibrary(authState.token, spacesState.selectedId)
    if (revision !== libraryLoadRevision) return false
    state.library = entries
    state.librarySnapshotReady = true
    return true
  } catch {
    if (revision === libraryLoadRevision) state.librarySnapshotReady = false
    // Non-fatal: retain the last visible snapshot, but never use it to prune
    // composition bindings after this failed refresh.
    return false
  }
}

/** Persist a partial local metadata update and replace the matching entry in-place. */

export async function updateLibraryMetadata(
  id: string,
  metadata: api.LibraryMetadataPatch,
): Promise<MediaEntry> {
  const updated = await api.patchLibraryMetadata(
    id,
    metadata,
    authState.token,
    spacesState.selectedId,
  )
  updateCachedLibrary(updated)
  state.library = state.library.map((entry) => (entry.id === id ? updated : entry))
  return updated
}


export async function loadCapabilities(): Promise<void> {
  try {
    state.capabilities = await api.getCapabilities()
    state.backendStatus = 'online'
  } catch (error) {
    // Older/offline backends keep the existing optimistic UI as a fallback.
    state.capabilities = null
    const proxyOutage = error instanceof api.ApiError && error.status >= 500 && !error.code
    state.backendStatus =
      error instanceof api.BackendUnavailableError || proxyOutage ? 'offline' : 'online'
  }
}


export function openFromLibrary(entry: MediaEntry): void {
  if (entry.kind !== 'source' || (entry.mediaType && entry.mediaType !== 'video')) return
  markRestoringProject(entry.id)
  const v: VideoInfo = {
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    duration: entry.duration ?? 0,
    width: entry.width ?? 0,
    height: entry.height ?? 0,
    title: entry.title ?? null,
    sizeBytes: entry.sizeBytes ?? null,
    mediaType: 'video',
  }
  state.video = v
  state.result = null
  const edit = defaultEdit()
  edit.trimEnd = v.duration
  edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
  edit.scale = { w: v.width, h: -2 }
  state.edit = edit
  resetHistory()
  // Restore any saved edit for this clip (overrides the defaults above).
  void restoreProject(v.id)
  toast('info', v.title ? `Открыто: ${v.title}` : 'Клип открыт')
}


export async function deleteFromLibrary(id: string): Promise<void> {
  try {
    await api.deleteLibraryItem(id, authState.token, spacesState.selectedId)
    state.library = state.library.filter((e) => e.id !== id)
    if (state.video?.id === id) state.video = null
    toast('info', 'Удалено')
  } catch (e) {
    toast('error', e instanceof Error ? e.message : String(e))
  }
}

