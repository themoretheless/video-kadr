import { describe, expect, it } from 'vitest'
import { PatchCommand } from './history'

interface State {
  crop: { x: number; y: number; w: number; h: number }
  speed: number
  label: string
}

const initial: State = {
  crop: { x: 0, y: 0, w: 100, h: 100 },
  speed: 1,
  label: 'clip',
}

describe('PatchCommand', () => {
  it('stores only changed fields and inverts exactly', () => {
    const after = { ...initial, crop: { x: 10, y: 20, w: 80, h: 70 } }
    const command = PatchCommand.between(initial, after, 'crop-drag')

    expect(command?.changedKeys).toEqual(['crop'])
    expect(command?.apply(initial)).toEqual(after)
    expect(command?.invert().apply(after)).toEqual(initial)
  })

  it('coalesces a drag while retaining the original before value', () => {
    const middle = { ...initial, crop: { ...initial.crop, x: 10 } }
    const after = { ...initial, crop: { ...initial.crop, x: 30 } }
    const first = PatchCommand.between(initial, middle, 'crop-drag')!
    const second = PatchCommand.between(middle, after, 'crop-drag')!
    const merged = first.merge(second)!

    expect(merged.apply(initial)).toEqual(after)
    expect(merged.invert().apply(after)).toEqual(initial)
    expect(first.merge(PatchCommand.between(middle, after, 'slider')!)).toBeNull()
  })
})
