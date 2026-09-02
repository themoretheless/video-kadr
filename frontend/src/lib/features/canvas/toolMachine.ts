export type CanvasToolState = 'idle' | 'captured' | 'dragging' | 'cancelled'

export interface PointerCaptureTarget {
  setPointerCapture?(pointerId: number): void
  releasePointerCapture?(pointerId: number): void
  hasPointerCapture?(pointerId: number): boolean
}

export interface ToolMachineSnapshot<TMode extends string> {
  readonly state: CanvasToolState
  readonly pointerId: number | null
  readonly mode: TMode | null
}

export class CanvasToolMachine<TMode extends string> {
  private state: CanvasToolState = 'idle'
  private pointerId: number | null = null
  private mode: TMode | null = null
  private target: PointerCaptureTarget | null = null

  snapshot(): ToolMachineSnapshot<TMode> {
    return { state: this.state, pointerId: this.pointerId, mode: this.mode }
  }

  begin(mode: TMode, pointerId: number, target: PointerCaptureTarget): ToolMachineSnapshot<TMode> {
    if (this.pointerId === pointerId && this.mode === mode && this.state !== 'idle') return this.snapshot()
    this.finish()
    this.mode = mode
    this.pointerId = pointerId
    this.target = target
    target.setPointerCapture?.(pointerId)
    this.state = 'captured'
    return this.snapshot()
  }

  move(pointerId: number): ToolMachineSnapshot<TMode> {
    if (pointerId !== this.pointerId || this.state === 'idle' || this.state === 'cancelled') return this.snapshot()
    this.state = 'dragging'
    return this.snapshot()
  }

  cancel(pointerId = this.pointerId): ToolMachineSnapshot<TMode> {
    if (pointerId == null || pointerId !== this.pointerId) return this.snapshot()
    this.state = 'cancelled'
    this.release()
    this.target = null
    return this.snapshot()
  }

  finish(pointerId = this.pointerId): ToolMachineSnapshot<TMode> {
    if (this.pointerId == null || (pointerId != null && pointerId !== this.pointerId)) return this.snapshot()
    this.release()
    this.state = 'idle'
    this.pointerId = null
    this.mode = null
    this.target = null
    return this.snapshot()
  }

  private release(): void {
    if (this.pointerId == null || !this.target) return
    if (!this.target.hasPointerCapture || this.target.hasPointerCapture(this.pointerId)) {
      this.target.releasePointerCapture?.(this.pointerId)
    }
  }
}
