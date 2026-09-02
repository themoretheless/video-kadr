import { describe, expect, it } from 'vitest'
import { TOOL_DESCRIPTORS, validateToolRegistry, type ToolDescriptor } from './registry.js'

describe('tool descriptor registry', () => {
  it('has stable unique ids and shortcuts', () => {
    expect(() => validateToolRegistry()).not.toThrow()
    expect(TOOL_DESCRIPTORS.map(({ id }) => id)).toEqual(['select', 'transform', 'hand', 'blade'])
  })

  it('fails closed on descriptor collisions', () => {
    const duplicate = [...TOOL_DESCRIPTORS, { ...TOOL_DESCRIPTORS[0] }] as readonly ToolDescriptor[]
    expect(() => validateToolRegistry(duplicate)).toThrow('Duplicate tool id')
  })
})
