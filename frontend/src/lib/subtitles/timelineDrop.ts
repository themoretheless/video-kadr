import { authState } from '$lib/state/auth.svelte'

const subtitleDragType = 'application/x-video-kadr-subtitle'
let active: { id: string; owner: string | null; insert: (time: number) => void } | undefined

export function startSubtitleDrag(event: DragEvent, insert: (time: number) => void): () => void {
  const transfer = event.dataTransfer
  if (!transfer) return () => {}
  const id = crypto.randomUUID()
  transfer.setData(subtitleDragType, id)
  transfer.effectAllowed = 'copy'
  const drag = { id, owner: authState.token, insert }
  active = drag
  return () => {
    if (active === drag) active = undefined
  }
}

export function acceptSubtitleDrag(event: DragEvent): boolean {
  if (!active || active.owner !== authState.token || !event.dataTransfer?.types.includes(subtitleDragType)) return false
  event.preventDefault()
  event.dataTransfer.dropEffect = 'copy'
  return true
}

export function dropSubtitle(event: DragEvent, time: number): void {
  if (!Number.isSafeInteger(time) || time < 0 || !acceptSubtitleDrag(event) || !active || event.dataTransfer?.getData(subtitleDragType) !== active.id) return
  const { insert } = active
  active = undefined
  insert(time)
}
