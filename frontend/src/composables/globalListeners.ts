import { onDestroy, onMount } from 'svelte'

export type ListenerBinding = readonly [
  target: EventTarget,
  type: string,
  listener: EventListenerOrEventListenerObject,
  options?: AddEventListenerOptions | boolean,
]

export function eventListener<T extends Event>(listener: (event: T) => void): EventListener {
  return listener as EventListener
}

export function listenMany(bindings: readonly ListenerBinding[]): () => void {
  for (const [target, type, listener, options] of bindings) {
    target.addEventListener(type, listener, options)
  }
  let active = true
  return () => {
    if (!active) return
    active = false
    for (const [target, type, listener, options] of bindings) {
      target.removeEventListener(type, listener, options)
    }
  }
}

export function useGlobalListeners(factory: () => readonly ListenerBinding[]): void {
  let dispose = () => {}
  onMount(() => {
    dispose = listenMany(factory())
  })
  onDestroy(() => dispose())
}
