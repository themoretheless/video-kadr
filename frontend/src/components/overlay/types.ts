// Shared shapes for the live preview overlay components.

/**
 * One interaction step on a preview box, in CSS pixels relative to the preview
 * element. A corner drag both moves and resizes, so all four fields travel
 * together.
 */
export interface BoxTransform {
  dx: number
  dy: number
  dw: number
  dh: number
}
