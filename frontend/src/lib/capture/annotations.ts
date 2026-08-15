export type AnnotationTool = 'pen' | 'highlighter' | 'rectangle' | 'arrow'

export interface AnnotationPoint {
  x: number
  y: number
}

export interface AnnotationStyle {
  color: string
  width: number
}

export interface AnnotationStroke extends AnnotationStyle {
  id: string
  tool: AnnotationTool
  points: AnnotationPoint[]
}

export interface AnnotationSnapshot {
  strokes: AnnotationStroke[]
  active: AnnotationStroke | null
  revision: number
}

export type AnnotationListener = (snapshot: AnnotationSnapshot) => void

export interface AnnotationReader {
  getSnapshot(): AnnotationSnapshot
}

function clamp(value: number): number {
  return Math.min(1, Math.max(0, Number.isFinite(value) ? value : 0))
}

function normalizedPoint(point: AnnotationPoint): AnnotationPoint {
  return { x: clamp(point.x), y: clamp(point.y) }
}

function cloneStroke(stroke: AnnotationStroke): AnnotationStroke {
  return { ...stroke, points: stroke.points.map((point) => ({ ...point })) }
}

function distance(a: AnnotationPoint, b: AnnotationPoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y)
}

export class AnnotationModel implements AnnotationReader {
  private strokes: AnnotationStroke[] = []
  private active: AnnotationStroke | null = null
  private revision = 0
  private nextId = 1
  private readonly listeners = new Set<AnnotationListener>()

  getSnapshot(): AnnotationSnapshot {
    return {
      strokes: this.strokes.map(cloneStroke),
      active: this.active ? cloneStroke(this.active) : null,
      revision: this.revision,
    }
  }

  subscribe(listener: AnnotationListener): () => void {
    this.listeners.add(listener)
    listener(this.getSnapshot())
    return () => this.listeners.delete(listener)
  }

  begin(tool: AnnotationTool, point: AnnotationPoint, style: AnnotationStyle): void {
    const start = normalizedPoint(point)
    this.active = {
      id: `annotation-${this.nextId++}`,
      tool,
      color: style.color,
      width: Math.min(48, Math.max(1, style.width)),
      points: [start],
    }
    this.changed()
  }

  append(point: AnnotationPoint): void {
    if (!this.active) return
    const next = normalizedPoint(point)
    const last = this.active.points.at(-1)
    if (last && distance(last, next) < 0.0005) return

    if (this.active.tool === 'pen' || this.active.tool === 'highlighter') {
      this.active.points.push(next)
    } else if (this.active.points.length === 1) {
      this.active.points.push(next)
    } else {
      this.active.points[1] = next
    }
    this.changed()
  }

  end(point?: AnnotationPoint): AnnotationStroke | null {
    if (!this.active) return null
    if (point) this.append(point)
    const completed = this.active
    this.active = null

    const start = completed.points[0]
    const finish = completed.points.at(-1)
    const isShape = completed.tool === 'rectangle' || completed.tool === 'arrow'
    if (isShape && (!start || !finish || distance(start, finish) < 0.002)) {
      this.changed()
      return null
    }

    this.strokes.push(completed)
    this.changed()
    return cloneStroke(completed)
  }

  cancel(): void {
    if (!this.active) return
    this.active = null
    this.changed()
  }

  undo(): AnnotationStroke | null {
    if (this.active) {
      const active = this.active
      this.active = null
      this.changed()
      return cloneStroke(active)
    }
    const removed = this.strokes.pop() ?? null
    if (removed) this.changed()
    return removed ? cloneStroke(removed) : null
  }

  clear(): void {
    if (!this.active && this.strokes.length === 0) return
    this.active = null
    this.strokes = []
    this.changed()
  }

  private changed(): void {
    this.revision += 1
    const snapshot = this.getSnapshot()
    this.listeners.forEach((listener) => listener(snapshot))
  }
}

function canvasPoint(point: AnnotationPoint, width: number, height: number): AnnotationPoint {
  return { x: point.x * width, y: point.y * height }
}

function drawFreehand(
  context: CanvasRenderingContext2D,
  stroke: AnnotationStroke,
  width: number,
  height: number,
): void {
  const first = stroke.points[0]
  if (!first) return
  const start = canvasPoint(first, width, height)
  context.beginPath()
  context.moveTo(start.x, start.y)
  if (stroke.points.length === 1) context.lineTo(start.x + 0.01, start.y + 0.01)
  for (const point of stroke.points.slice(1)) {
    const next = canvasPoint(point, width, height)
    context.lineTo(next.x, next.y)
  }
  context.stroke()
}

function drawRectangle(
  context: CanvasRenderingContext2D,
  stroke: AnnotationStroke,
  width: number,
  height: number,
): void {
  const first = stroke.points[0]
  const last = stroke.points.at(-1)
  if (!first || !last) return
  const start = canvasPoint(first, width, height)
  const end = canvasPoint(last, width, height)
  context.strokeRect(start.x, start.y, end.x - start.x, end.y - start.y)
}

function drawArrow(
  context: CanvasRenderingContext2D,
  stroke: AnnotationStroke,
  width: number,
  height: number,
): void {
  const first = stroke.points[0]
  const last = stroke.points.at(-1)
  if (!first || !last) return
  const start = canvasPoint(first, width, height)
  const end = canvasPoint(last, width, height)
  const angle = Math.atan2(end.y - start.y, end.x - start.x)
  const headLength = Math.max(12, context.lineWidth * 4)

  context.beginPath()
  context.moveTo(start.x, start.y)
  context.lineTo(end.x, end.y)
  context.moveTo(end.x, end.y)
  context.lineTo(end.x - headLength * Math.cos(angle - Math.PI / 6), end.y - headLength * Math.sin(angle - Math.PI / 6))
  context.moveTo(end.x, end.y)
  context.lineTo(end.x - headLength * Math.cos(angle + Math.PI / 6), end.y - headLength * Math.sin(angle + Math.PI / 6))
  context.stroke()
}

export function renderAnnotations(
  context: CanvasRenderingContext2D,
  snapshot: AnnotationSnapshot,
  width: number,
  height: number,
): void {
  const strokes = snapshot.active ? [...snapshot.strokes, snapshot.active] : snapshot.strokes
  const scale = Math.max(0.5, Math.min(width, height) / 720)

  for (const stroke of strokes) {
    context.save()
    context.strokeStyle = stroke.color
    context.lineWidth = stroke.width * scale
    context.lineCap = 'round'
    context.lineJoin = 'round'
    context.globalAlpha = stroke.tool === 'highlighter' ? 0.32 : 1

    if (stroke.tool === 'rectangle') drawRectangle(context, stroke, width, height)
    else if (stroke.tool === 'arrow') drawArrow(context, stroke, width, height)
    else drawFreehand(context, stroke, width, height)
    context.restore()
  }
}
