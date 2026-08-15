import { describe, expect, it, vi } from 'vitest'
import { AnnotationModel } from './annotations.js'

describe('AnnotationModel', () => {
  it('records normalized freehand strokes and publishes immutable snapshots', () => {
    const model = new AnnotationModel()
    const listener = vi.fn()
    model.subscribe(listener)

    model.begin('pen', { x: -1, y: 0.25 }, { color: '#ff0000', width: 6 })
    model.append({ x: 0.5, y: 0.75 })
    model.append({ x: 2, y: 1 })
    const completed = model.end()

    expect(completed).toMatchObject({ tool: 'pen', color: '#ff0000', width: 6 })
    expect(completed?.points).toEqual([
      { x: 0, y: 0.25 },
      { x: 0.5, y: 0.75 },
      { x: 1, y: 1 },
    ])
    const snapshot = model.getSnapshot()
    snapshot.strokes[0]?.points.push({ x: 0, y: 0 })
    expect(model.getSnapshot().strokes[0]?.points).toHaveLength(3)
    expect(listener).toHaveBeenCalled()
  })

  it('keeps only the latest endpoint for shapes and discards accidental clicks', () => {
    const model = new AnnotationModel()
    model.begin('rectangle', { x: 0.1, y: 0.2 }, { color: '#00ff00', width: 3 })
    model.append({ x: 0.5, y: 0.6 })
    model.append({ x: 0.8, y: 0.9 })
    model.end()
    expect(model.getSnapshot().strokes[0]?.points).toEqual([
      { x: 0.1, y: 0.2 },
      { x: 0.8, y: 0.9 },
    ])

    model.begin('arrow', { x: 0.3, y: 0.3 }, { color: '#ffffff', width: 2 })
    expect(model.end()).toBeNull()
    expect(model.getSnapshot().strokes).toHaveLength(1)
  })

  it('undoes the active or latest annotation and clears the document', () => {
    const model = new AnnotationModel()
    model.begin('highlighter', { x: 0, y: 0 }, { color: '#ffff00', width: 14 })
    model.append({ x: 1, y: 1 })
    model.end()
    model.begin('pen', { x: 0.5, y: 0.5 }, { color: '#ffffff', width: 2 })

    expect(model.undo()?.tool).toBe('pen')
    expect(model.getSnapshot().strokes).toHaveLength(1)
    expect(model.undo()?.tool).toBe('highlighter')
    expect(model.getSnapshot().strokes).toHaveLength(0)

    model.begin('pen', { x: 0, y: 0 }, { color: '#ffffff', width: 2 })
    model.end()
    model.clear()
    expect(model.getSnapshot()).toMatchObject({ strokes: [], active: null })
  })
})
