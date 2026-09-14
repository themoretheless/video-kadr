import * as api from '../../api.js'
import {
  authState
} from '../auth.svelte.js'
import {
  spacesState
} from '../spaces.svelte.js'
import {
  editorFacade
} from '../../../stores/editorFacade.js'
import {
  cacheCompositionProject,
  fetchCompositionProject,
  fetchCompositionProjects,
  removeCompositionProject
} from '../../../data/projects.js'
import {
  normalizeComposition
} from '../../composition/validation.js'
import {
  DEFAULT_COMPOSITION_RENDER_OUTPUT,
  PROJECT_BUSY_MESSAGE,
  activateStoredDraft,
  activeDraftDirty,
  cloneComposition,
  compositionProjectsEtag,
  setActiveDraftDirty,
  setCompositionProjectsEtag,
  compositionRenderOutput,
  compositionState,
  createBlankStoredDraft,
  bumpOpenCompositionRevision,
  isCurrentOpenCompositionRevision,
  nextOpenCompositionRevision,
  setProjectWriteBusy,
  persistCurrentDraftNow,
  projectWriteBusy,
  readStoredDraftCollection,
  removeStoredProjectDraft
} from './core.svelte.js'

export async function loadCompositionProjects(): Promise<void> {
  try {
    setCompositionProjectsEtag(null)
    const projects = await fetchCompositionProjects()
    editorFacade.replaceProjects(projects)
    compositionState.projects = [...editorFacade.projects]
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  }
}

/** Refresh the complete visible project list so background changes and deletes appear. */

export async function refreshCompositionProjects(): Promise<'unchanged' | 'updated'> {
  const token = authState.token
  if (!token || compositionState.save.busy) return 'unchanged'
  try {
    const snapshot = await api.getCompositionProjectsIfChanged(token, compositionProjectsEtag)
    if (!snapshot) return 'unchanged'
    setCompositionProjectsEtag(snapshot.etag)
    editorFacade.replaceProjects(snapshot.projects)
    compositionState.projects = [...editorFacade.projects]
    return 'updated'
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
    return 'unchanged'
  }
}


export async function saveCompositionProject(): Promise<void> {
  if (compositionState.save.busy) return
  persistCurrentDraftNow()
  setProjectWriteBusy(true)
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    const token = authState.token
    if (!token) throw new Error('Войдите, чтобы сохранить проект')
    const body = {
      name: compositionState.projectName.trim() || undefined,
      spaceId: compositionState.projectId ? undefined : spacesState.selectedId || undefined,
      baseRevision: compositionState.projectId
        ? compositionState.projectRevision ?? undefined
        : undefined,
      document: cloneComposition(compositionState.document),
    }
    const project = compositionState.projectId
      ? await api.updateCompositionProject(compositionState.projectId, body, token)
      : await api.createCompositionProject(body, token)
    cacheCompositionProject(project)
    editorFacade.projectSaved(project)
    const wasUnsaved = compositionState.projectId === null
    compositionState.projectId = project.id
    compositionState.projectRevision = project.revision
    compositionState.projectName = project.name
    compositionState.projects = [project, ...compositionState.projects.filter((item) => item.id !== project.id)]
    setActiveDraftDirty(false)
    persistCurrentDraftNow(wasUnsaved)
    compositionState.save.error = ''
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    setProjectWriteBusy(false)
    compositionState.save.busy = false
  }
}


export async function openCompositionProject(id: string): Promise<void> {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    return
  }
  persistCurrentDraftNow()
  const revision = nextOpenCompositionRevision()
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    const project = await fetchCompositionProject(id)
    if (!isCurrentOpenCompositionRevision(revision)) return
    if (!project) throw new Error('Композиционный проект не найден')
    editorFacade.selectProject(project.id)
    const local = readStoredDraftCollection()?.projects[project.id]
    activateStoredDraft(local?.dirty ? local : {
      document: normalizeComposition(project.document),
      media: local?.media ?? {},
      projectId: project.id,
      projectRevision: project.revision,
      projectName: project.name,
      exportSettings: local?.exportSettings ?? DEFAULT_COMPOSITION_RENDER_OUTPUT,
      dirty: false,
    })
    persistCurrentDraftNow()
  } catch (error) {
    if (!isCurrentOpenCompositionRevision(revision)) return
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    if (isCurrentOpenCompositionRevision(revision)) compositionState.save.busy = false
  }
}


export type RemoteProjectRefresh = 'unchanged' | 'updated' | 'conflict' | 'deleted'

/** Refresh the active project without ever overwriting local unsaved edits. */

export async function refreshOpenCompositionProject(): Promise<RemoteProjectRefresh> {
  const id = compositionState.projectId
  const token = authState.token
  const revision = compositionState.projectRevision
  if (!id || !token || compositionState.save.busy) return 'unchanged'
  try {
    const project = await api.getCompositionProjectIfChanged(id, token, revision)
    if (compositionState.projectId !== id || compositionState.projectRevision !== revision) {
      return 'unchanged'
    }
    if (project === undefined) return 'unchanged'
    if (project === null) {
      compositionState.save.error = 'Проект удалён на другом устройстве'
      return 'deleted'
    }
    cacheCompositionProject(project)
    compositionState.projects = [
      project,
      ...compositionState.projects.filter((item) => item.id !== project.id),
    ]
    if (activeDraftDirty) {
      compositionState.save.error =
        'Проект изменён на другом устройстве. Локальные правки сохранены; обновите проект перед повторным сохранением.'
      return 'conflict'
    }
    const local = readStoredDraftCollection()?.projects[id]
    activateStoredDraft({
      document: normalizeComposition(project.document),
      media: local?.media ?? compositionState.media,
      projectId: project.id,
      projectRevision: project.revision,
      projectName: project.name,
      exportSettings: local?.exportSettings ?? compositionRenderOutput(),
      dirty: false,
    })
    compositionState.ui.message = 'Проект обновлён с другого устройства'
    compositionState.save.error = ''
    persistCurrentDraftNow()
    return 'updated'
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
    return 'unchanged'
  }
}


export async function deleteCompositionProject(id: string): Promise<void> {
  if (compositionState.save.busy) return
  setProjectWriteBusy(true)
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    await removeCompositionProject(id)
    editorFacade.projectDeleted(id)
    compositionState.projects = compositionState.projects.filter((project) => project.id !== id)
    removeStoredProjectDraft(id)
    if (compositionState.projectId === id) {
      bumpOpenCompositionRevision()
      const unsaved = readStoredDraftCollection()?.unsaved
      activateStoredDraft(unsaved ?? createBlankStoredDraft())
      persistCurrentDraftNow()
    }
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    setProjectWriteBusy(false)
    compositionState.save.busy = false
  }
}

