import { onDestroy } from 'svelte'

export function observeResize(
  target: Element,
  callback: ResizeObserverCallback,
  factory: typeof ResizeObserver = ResizeObserver,
): () => void {
  const observer = new factory(callback)
  observer.observe(target)
  return () => observer.disconnect()
}

export function useResizeObserver(target: () => Element | null, callback: ResizeObserverCallback): void {
  let dispose = () => {}
  $effect(() => {
    dispose()
    const element = target()
    dispose = element ? observeResize(element, callback) : () => {}
    return dispose
  })
  onDestroy(() => dispose())
}
