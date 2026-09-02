import { beforeEach, describe, expect, it } from 'vitest'
import { editorFacade } from './editorFacade.js'

const project = (id: string) => ({
  id,
  name: id,
  schemaVersion: 2 as const,
  mode: 'composition' as const,
  document: {} as never,
  sourceIds: [],
  createdAt: 1,
  updatedAt: 1,
})

describe('editor facade', () => {
  beforeEach(() => {
    editorFacade.replaceProjects([])
    editorFacade.reportError('')
  })

  it('coordinates project and UI stores through commands', () => {
    editorFacade.begin('save')
    editorFacade.projectSaved(project('p1'))
    expect(editorFacade.busy).toBe(true)
    expect(editorFacade.selectedProjectId).toBe('p1')
    expect(editorFacade.projects.map((item) => item.id)).toEqual(['p1'])
    editorFacade.finish('save')
    expect(editorFacade.busy).toBe(false)
  })

  it('clears selection when a project is deleted', () => {
    editorFacade.projectSaved(project('p2'))
    editorFacade.projectDeleted('p2')
    expect(editorFacade.selectedProjectId).toBeNull()
  })
})
