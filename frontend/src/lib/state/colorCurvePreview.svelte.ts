import type { Composition } from '$lib/composition/types'
export const colorCurvePreview = $state<{ owner: number | null;source: Composition | null;document: Composition | null }>({ owner: null,source: null,document: null })
let nextOwner = 0
export function colorCurvePreviewOwner(): number { return ++nextOwner }
export function clearColorCurvePreview(owner: number): void {
  if (colorCurvePreview.owner !== owner) return
  colorCurvePreview.owner = null;colorCurvePreview.source = null;colorCurvePreview.document = null
}
