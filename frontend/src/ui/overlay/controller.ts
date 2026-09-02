import {
  autoUpdate,
  computePosition,
  flip,
  offset,
  shift,
  type Middleware,
  type Placement,
} from '@floating-ui/dom'

export function overlayMiddleware(): Middleware[] {
  return [offset(8), flip(), shift({ padding: 8 })]
}

export function positionOverlay(
  anchor: Element,
  floating: HTMLElement,
  placement: Placement,
  apply: (position: { x: number; y: number; placement: Placement }) => void,
): () => void {
  return autoUpdate(anchor, floating, () => {
    void computePosition(anchor, floating, {
      placement,
      middleware: overlayMiddleware(),
    }).then(({ x, y, placement: resolvedPlacement }) => {
      apply({ x, y, placement: resolvedPlacement })
    })
  })
}
