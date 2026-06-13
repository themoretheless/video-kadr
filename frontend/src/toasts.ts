import { reactive } from 'vue'

export type ToastKind = 'error' | 'success' | 'info'

export interface Toast {
  id: number
  kind: ToastKind
  text: string
}

let nextId = 1

export const toasts = reactive<Toast[]>([])

/** Show a toast; it auto-dismisses after 4 seconds or on click. */
export function toast(kind: ToastKind, text: string): void {
  const id = nextId++
  toasts.push({ id, kind, text })
  setTimeout(() => dismissToast(id), 4000)
}

export function dismissToast(id: number): void {
  const i = toasts.findIndex((t) => t.id === id)
  if (i !== -1) toasts.splice(i, 1)
}
