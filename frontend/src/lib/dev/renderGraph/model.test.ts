import { describe, expect, it } from 'vitest'
import { topologicalRenderOrder } from './model.js'

describe('render DAG visualizer model', () => {
  it('sorts dependencies before consumers deterministically', () => {
    expect(topologicalRenderOrder([
      { id: 'export', label: 'Export', dependencies: ['grade'] },
      { id: 'decode', label: 'Decode', dependencies: [] },
      { id: 'grade', label: 'Grade', dependencies: ['decode'] },
    ])).toEqual(['decode', 'grade', 'export'])
  })

  it('rejects cycles', () => {
    expect(() => topologicalRenderOrder([
      { id: 'a', label: 'A', dependencies: ['b'] },
      { id: 'b', label: 'B', dependencies: ['a'] },
    ])).toThrow('cycle')
  })
})
