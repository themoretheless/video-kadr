import { describe, expect, it } from 'vitest'
import { createComposition } from './commands'
import { patchCompositionCanvas } from './canvas'
import { evaluateAgentBatch } from '../agent/operations'

describe('shared canvas edits', () => {
  it('matches the native unsigned 32-bit exact-rate contract before publishing a patch', () => {
    const document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    const largest = patchCompositionCanvas(document, { fps: 1, fpsRate: { numerator: 0xFFFFFFFF, denominator: 0xFFFFFFFF } })
    expect(largest.canvas.fpsRate?.numerator).toBe(0xFFFFFFFF)
    const before = JSON.stringify(document)
    expect(() => patchCompositionCanvas(document, { fps: 1, fpsRate: { numerator: 0x100000000, denominator: 0x100000000 } })).toThrow('32-bit')
    expect(() => evaluateAgentBatch(document, [{ op: 'update_canvas', patch: { fps: 0x100000000 / 143165576, fpsRate: { numerator: 0x100000000, denominator: 143165576 } } }])).toThrow('32-bit')
    expect(JSON.stringify(document)).toBe(before)
  })

  it('preserves exact FPS on unrelated edits and clears it on numeric changes through agent batches', () => {
    const original = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    const exact = evaluateAgentBatch(original, [{ op: 'update_canvas', patch: { fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 } } }, { op: 'update_canvas', patch: { width: 1920 } }])
    expect(exact.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    expect(exact.canvas.width).toBe(1920)
    expect(patchCompositionCanvas(exact, { fps: 30 }).canvas.fpsRate).toBeUndefined()
    expect(original.canvas.fps).toBe(30)
  })
  it('rejects malformed canvas metadata and failed later operations without modifying the input', () => {
    const document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    const before = JSON.stringify(document)
    expect(() => evaluateAgentBatch(document, [{ op: 'update_canvas', patch: { width: 1920 } }, { op: 'update_canvas', patch: { fpsRate: { numerator: 30000, denominator: 1001 } } }])).toThrow()
    expect(() => patchCompositionCanvas(document, { unknown: 1 } as never)).toThrow('patch')
    expect(JSON.stringify(document)).toBe(before)
  })
})
