export type ToastKind = 'error' | 'success' | 'info'

export interface Toast {
  id: number
  kind: ToastKind
  text: string
}

let nextId = 1

export const toasts = $state<Toast[]>([])

/** Show a toast; it auto-dismisses after four seconds or on click. */
export function toast(kind: ToastKind, text: string): void {
  const id = nextId++
  toasts.push({ id, kind, text })
  setTimeout(() => dismissToast(id), 4000)
}

export function dismissToast(id: number): void {
  const index = toasts.findIndex((candidate) => candidate.id === id)
  if (index !== -1) toasts.splice(index, 1)
}
