import {
  assertValidComposition,
  normalizeComposition
} from '../../composition/validation.js'
import type {
  Composition
} from '../../composition/types.js'
import {
  DEFAULT_COMPOSITION_RENDER_OUTPUT,
  MODE_KEY,
  PROJECT_BUSY_MESSAGE,
  UNSAVED_GUARD_MESSAGE,
  activateStoredDraft,
  activeDraftDirty,
  commitDocument,
  compositionState,
  createBlankStoredDraft,
  editorMode,
  getStorage,
  bumpOpenCompositionRevision,
  persistCurrentDraftNow,
  projectWriteBusy,
  readStoredDraftCollection,
  replaceDocument,
  scheduleAutosave
} from './core.svelte.js'
import type {
  EditorMode
} from './core.svelte.js'

export function setEditorMode(mode: EditorMode): void {
  editorMode.value = mode
  getStorage()?.setItem(MODE_KEY, mode)
  if (mode === 'legacy') compositionState.transport.playing = false
}


export function newComposition(): void {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    return
  }
  if (compositionState.projectId === null && activeDraftDirty) {
    persistCurrentDraftNow()
    compositionState.save.error = UNSAVED_GUARD_MESSAGE
    return
  }

  bumpOpenCompositionRevision()
  persistCurrentDraftNow()
  const unsaved = readStoredDraftCollection()?.unsaved
  activateStoredDraft(unsaved ?? createBlankStoredDraft())
  compositionState.save.busy = false
  compositionState.save.error = ''
  persistCurrentDraftNow()
}


export function setCompositionProjectName(name: string): void {
  compositionState.projectName = name.slice(0, 256)
  scheduleAutosave()
}


export function replaceCompositionDocument(document: Composition): void {
  replaceDocument(normalizeComposition(document), false)
}


export function openCompositionDocumentAsNew(document: Composition, name = 'Новая композиция'): void {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    throw new Error(PROJECT_BUSY_MESSAGE)
  }
  document = normalizeComposition(document)
  persistCurrentDraftNow()
  const storedUnsaved = readStoredDraftCollection()?.unsaved
  if (
    (compositionState.projectId === null && activeDraftDirty) ||
    (compositionState.projectId !== null && storedUnsaved?.dirty)
  ) {
    compositionState.save.error = UNSAVED_GUARD_MESSAGE
    throw new Error(UNSAVED_GUARD_MESSAGE)
  }

  bumpOpenCompositionRevision()
  activateStoredDraft({
    document,
    media: {},
    projectId: null,
    projectName: name.trim().slice(0, 256) || 'Новая композиция',
    exportSettings: DEFAULT_COMPOSITION_RENDER_OUTPUT,
    dirty: true,
  })
  persistCurrentDraftNow()
}


export function updateCompositionCanvas(patch: Partial<Composition['canvas']>): void {
  const canvas = { ...compositionState.document.canvas, ...patch }
  const document = { ...compositionState.document, canvas }
  assertValidComposition(document)
  commitDocument(document)
}

