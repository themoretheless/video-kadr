import type { Composition } from './types'
import { assertValidComposition } from './validation'

export function patchCompositionCanvas(document: Composition, patch: Partial<Composition['canvas']>): Composition {
  if (!patch || typeof patch !== 'object' || Array.isArray(patch) || Object.keys(patch).some((key) => !['width', 'height', 'fps', 'fpsRate', 'backgroundColor', 'backgroundMode', 'backgroundBlur'].includes(key))) throw new Error('Invalid canvas patch')
  const canvas = { ...document.canvas, ...(Object.hasOwn(patch, 'fps') && !Object.hasOwn(patch, 'fpsRate') ? { fpsRate: undefined } : {}), ...patch }
  const next = { ...document, canvas }
  assertValidComposition(next)
  return next
}
