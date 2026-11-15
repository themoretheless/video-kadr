import { authState } from '$lib/state/auth.svelte'

let preview: { receiver: EventTarget | null; marker: HTMLElement } | undefined

function clearPreview(): void {
  preview?.marker.removeAttribute('data-subtitle-drop-preview')
  preview?.marker.style.removeProperty('--subtitle-drop-position')
  preview = undefined
}

const subtitleDragType = 'application/x-video-kadr-subtitle'
let active: { id: string; owner: string | null; insert: (time: number) => void } | undefined

export function startSubtitleDrag(event: DragEvent, insert: (time: number) => void): () => void {
  const transfer = event.dataTransfer
  if (!transfer) return () => {}
  const id = crypto.randomUUID()
  transfer.setData(subtitleDragType, id)
  transfer.effectAllowed = 'copy'
  const drag = { id, owner: authState.token, insert }
  clearPreview()
  active = drag
  return () => {
    if (active !== drag) return
    clearPreview()
    active = undefined
  }
}

function acceptSubtitleDrag(event: DragEvent): boolean {
  if (!active || active.owner !== authState.token || !event.dataTransfer?.types.includes(subtitleDragType)) return false
  event.preventDefault()
  event.dataTransfer.dropEffect = 'copy'
  return true
}

export function previewSubtitleDrop(event: DragEvent, position: string, marker: HTMLElement): void {
  if (!acceptSubtitleDrag(event)) return
  if (preview?.marker !== marker) clearPreview()
  preview = { receiver: event.currentTarget, marker }
  marker.style.setProperty('--subtitle-drop-position', position)
  marker.setAttribute('data-subtitle-drop-preview', '')
}

export function leaveSubtitleDrop(event: DragEvent): void {
  if (preview?.receiver !== event.currentTarget) return
  const receiver = event.currentTarget
  if (receiver instanceof Node && event.relatedTarget instanceof Node && receiver.contains(event.relatedTarget)) return
  clearPreview()
}

export function dropSubtitle(event: DragEvent, time: number): void {
  if (!Number.isSafeInteger(time) || time < 0 || !acceptSubtitleDrag(event) || !active || event.dataTransfer?.getData(subtitleDragType) !== active.id) return
  clearPreview()
  const { insert } = active
  active = undefined
  insert(time)
}
