<script lang="ts">
  import type { Placement } from '@floating-ui/dom'
  import type { Snippet } from 'svelte'
  import { eventListener, listenMany } from '../../composables/globalListeners.js'
  import { positionOverlay } from './controller.js'

  interface Props {
    anchor: HTMLElement | null
    open: boolean
    kind?: 'tooltip' | 'menu' | 'popover'
    placement?: Placement
    label?: string
    onclose?: () => void
    children: Snippet
  }

  let {
    anchor,
    open,
    kind = 'popover',
    placement = 'bottom-start',
    label = 'Всплывающее окно',
    onclose,
    children,
  }: Props = $props()
  let floating = $state<HTMLElement>()
  let style = $state('')
  let resolvedPlacement = $state<Placement>('bottom-start')

  function closeAndRestoreFocus(): void {
    onclose?.()
    queueMicrotask(() => anchor?.focus())
  }

  $effect(() => {
    if (!open || !anchor || !floating) return
    const currentFloating = floating
    const disposePosition = positionOverlay(anchor, currentFloating, placement, ({ x, y, placement: resolved }) => {
      style = `left:${x}px;top:${y}px`
      resolvedPlacement = resolved
    })
    const disposeListeners = listenMany([
      [window, 'keydown', eventListener((event: KeyboardEvent) => {
        if (event.key === 'Escape') {
          event.preventDefault()
          closeAndRestoreFocus()
        }
      })],
      [document, 'pointerdown', eventListener((event: PointerEvent) => {
        const target = event.target as Node | null
        if (target && !currentFloating.contains(target) && !anchor.contains(target)) closeAndRestoreFocus()
      })],
    ])
    if (kind !== 'tooltip') queueMicrotask(() => currentFloating.focus())
    return () => {
      disposeListeners()
      disposePosition()
    }
  })
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    bind:this={floating}
    class="overlay"
    style={style}
    role={kind === 'popover' ? 'dialog' : kind}
    aria-label={label}
    aria-modal="false"
    tabindex={kind === 'tooltip' ? undefined : -1}
    data-placement={resolvedPlacement}
  >
    {@render children()}
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    z-index: 1000;
    max-width: min(24rem, calc(100vw - 16px));
    padding: 10px 12px;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
</style>
