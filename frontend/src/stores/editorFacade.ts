import type { CompositionProjectDto } from '$lib/api.js'
import { projectStore } from './project.svelte.js'
import { uiStore } from './ui.svelte.js'

/** The only cross-store coordinator; stores never import or mutate each other. */
export const editorFacade = {
  get projects(): readonly CompositionProjectDto[] {
    return projectStore.projects
  },
  get selectedProjectId(): string | null {
    return projectStore.selectedId
  },
  get busy(): boolean {
    return uiStore.busy
  },
  get message(): string {
    return uiStore.message
  },
  replaceProjects(projects: CompositionProjectDto[]): void {
    projectStore.replace(projects)
  },
  projectSaved(project: CompositionProjectDto): void {
    projectStore.saved(project)
    uiStore.report('')
  },
  selectProject(id: string | null): void {
    projectStore.select(id)
  },
  projectDeleted(id: string): void {
    projectStore.remove(id)
  },
  begin(scope: string): void {
    uiStore.begin(scope)
  },
  finish(scope: string): void {
    uiStore.finish(scope)
  },
  reportError(error: unknown): void {
    uiStore.report(error instanceof Error ? error.message : String(error))
  },
}
