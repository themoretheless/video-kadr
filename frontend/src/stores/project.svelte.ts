import type { CompositionProjectDto } from '$lib/api.js'

class ProjectStore {
  #projects = $state<CompositionProjectDto[]>([])
  #selectedId = $state<string | null>(null)

  get projects(): readonly CompositionProjectDto[] {
    return this.#projects
  }

  get selectedId(): string | null {
    return this.#selectedId
  }

  replace(projects: CompositionProjectDto[]): void {
    this.#projects = [...projects]
    if (this.#selectedId && !projects.some((project) => project.id === this.#selectedId)) {
      this.#selectedId = null
    }
  }

  saved(project: CompositionProjectDto): void {
    this.#projects = [project, ...this.#projects.filter((item) => item.id !== project.id)]
    this.#selectedId = project.id
  }

  select(id: string | null): void {
    this.#selectedId = id
  }

  remove(id: string): void {
    this.#projects = this.#projects.filter((project) => project.id !== id)
    if (this.#selectedId === id) this.#selectedId = null
  }
}

export const projectStore = new ProjectStore()
