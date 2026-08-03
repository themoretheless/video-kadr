import {
  clientOnlyMode,
  exportProjectArchive,
  importProjectArchive,
  type ProjectArchiveOptions,
  type ProjectDto,
} from './api'
import {
  exportBrowserProjectArchive,
  importBrowserProjectArchive,
  type BrowserProjectArchiveProgress,
} from './browser-project-archive'

export type { ProjectDto } from './api'
export type ProjectArchiveRuntimeProgress = BrowserProjectArchiveProgress

export interface ProjectArchiveRuntimeExportResult {
  blob: Blob
  filename: string
  warnings: string[]
}

export interface ProjectArchiveRuntimeImportResult {
  projectId: string
  project?: ProjectDto
  importedMedia: number
  reusedMedia: number
  missingMedia: string[]
  warnings: string[]
}

interface ArchiveControl {
  signal?: AbortSignal
  onProgress?: (progress: ProjectArchiveRuntimeProgress) => void
}

export async function exportPortableProjectArchive(
  project: ProjectDto,
  options: ProjectArchiveOptions,
  control: ArchiveControl = {},
): Promise<ProjectArchiveRuntimeExportResult> {
  if (clientOnlyMode) return exportBrowserProjectArchive(project, options, control)
  control.onProgress?.({
    phase: 'preparing', completedBytes: 0, totalBytes: 0, message: 'Сервер собирает архив…',
  })
  const result = await exportProjectArchive(project.id, options, control.signal)
  control.onProgress?.({
    phase: 'done', completedBytes: result.blob.size, totalBytes: result.blob.size, message: 'Архив готов',
  })
  return { ...result, warnings: [] }
}

export async function importPortableProjectArchive(
  archive: Blob,
  control: ArchiveControl = {},
): Promise<ProjectArchiveRuntimeImportResult> {
  if (clientOnlyMode) {
    const result = await importBrowserProjectArchive(archive, control)
    return { ...result, projectId: result.project.id }
  }
  control.onProgress?.({
    phase: 'verifying', completedBytes: 0, totalBytes: archive.size,
    message: 'Загружаю и проверяю архив на сервере…',
  })
  const result = await importProjectArchive(archive, control.signal)
  control.onProgress?.({
    phase: 'done', completedBytes: archive.size, totalBytes: archive.size, message: 'Проект импортирован',
  })
  return {
    projectId: result.projectId,
    importedMedia: 0,
    reusedMedia: 0,
    missingMedia: result.missingMedia,
    warnings: [],
  }
}
