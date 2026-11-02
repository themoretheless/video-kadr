import type { Composition } from '$lib/composition/types'
export const colorKeyDragPreview = $state<{ owner: number | null;source: Composition | null;edits: readonly { clipId: string;tick: number;newTick: number }[] }>({ owner: null,source: null,edits: [] })
export function clearColorKeyDragPreview(owner: number): void {
  if (colorKeyDragPreview.owner !== owner) return
  colorKeyDragPreview.owner = null;colorKeyDragPreview.source = null;colorKeyDragPreview.edits = []
}
