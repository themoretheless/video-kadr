import { describe, expect, it, vi } from 'vitest'
import { buildCanvasScene, hitTestCandidates } from './scene.js'
import { CanvasToolMachine } from './toolMachine.js'

describe('canvas contracts', () => {
  it('owns pointer capture idempotently and ignores foreign pointer races', () => {
    const target = {
      setPointerCapture: vi.fn(),
      releasePointerCapture: vi.fn(),
      hasPointerCapture: vi.fn(() => true),
    }
    const machine = new CanvasToolMachine<'move' | 'resize'>()
    expect(machine.begin('move', 7, target).state).toBe('captured')
    machine.begin('move', 7, target)
    expect(target.setPointerCapture).toHaveBeenCalledTimes(1)
    expect(machine.move(8).state).toBe('captured')
    expect(machine.move(7).state).toBe('dragging')
    machine.finish(8)
    expect(target.releasePointerCapture).not.toHaveBeenCalled()
    expect(machine.finish(7).state).toBe('idle')
    expect(target.releasePointerCapture).toHaveBeenCalledWith(7)
    machine.finish(7)
    expect(target.releasePointerCapture).toHaveBeenCalledTimes(1)

    machine.begin('resize', 9, target)
    expect(machine.cancel(9).state).toBe('cancelled')
    machine.finish(9)
    expect(target.releasePointerCapture).toHaveBeenCalledTimes(2)
  })

  it('keeps media and guides out of hit testing', () => {
    const scene = buildCanvasScene([
      { id: 'video', layer: 'media', interactive: false },
      { id: 'safe-area', layer: 'guides', interactive: false },
      { id: 'crop', layer: 'overlays', interactive: true },
      { id: 'se', layer: 'handles', interactive: true },
    ])
    expect(hitTestCandidates(scene).map((node) => node.id)).toEqual(['se', 'crop'])
  })
})
