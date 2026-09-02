import * as api from '$lib/api.js'
import { queryClient, serverKeys } from './queryClient.js'

export async function fetchCompositionProjects(): Promise<api.CompositionProjectDto[]> {
  const projects = await api.getCompositionProjects()
  queryClient.setQueryData(serverKeys.compositionProjects, projects)
  return projects
}

export async function fetchCompositionProject(id: string): Promise<api.CompositionProjectDto | null> {
  const project = await api.getCompositionProject(id)
  queryClient.setQueryData(serverKeys.compositionProject(id), project)
  return project
}

export function cacheCompositionProject(project: api.CompositionProjectDto): void {
  queryClient.setQueryData(serverKeys.compositionProject(project.id), project)
  queryClient.setQueryData<api.CompositionProjectDto[]>(
    serverKeys.compositionProjects,
    (current = []) => [project, ...current.filter((item) => item.id !== project.id)],
  )
}

export async function removeCompositionProject(id: string): Promise<void> {
  await api.deleteCompositionProject(id)
  queryClient.removeQueries({ queryKey: serverKeys.compositionProject(id) })
  queryClient.setQueryData<api.CompositionProjectDto[]>(
    serverKeys.compositionProjects,
    (current = []) => current.filter((item) => item.id !== id),
  )
}
