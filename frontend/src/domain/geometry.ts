export type CoordinateSpace = 'source' | 'preview' | 'export' | 'normalized'
export type SourceSpace = 'source'
export type PreviewSpace = 'preview'
export type ExportSpace = 'export'
export type NormalizedSpace = 'normalized'

declare const pointSpace: unique symbol
declare const rectSpace: unique symbol

export type Point<S extends CoordinateSpace> = Readonly<{
  x: number
  y: number
  [pointSpace]: S
}>

export type Rect<S extends CoordinateSpace> = Readonly<{
  x: number
  y: number
  width: number
  height: number
  [rectSpace]: S
}>

export function point<S extends CoordinateSpace>(x: number, y: number): Point<S> {
  assertFinite([x, y])
  return { x, y } as Point<S>
}

export function rect<S extends CoordinateSpace>(
  x: number,
  y: number,
  width: number,
  height: number,
): Rect<S> {
  assertFinite([x, y, width, height])
  if (width < 0 || height < 0) throw new GeometryError('Rectangle size cannot be negative')
  return { x, y, width, height } as Rect<S>
}

export class Transform2D<From extends CoordinateSpace, To extends CoordinateSpace> {
  readonly values: readonly [number, number, number, number, number, number]

  private constructor(values: readonly [number, number, number, number, number, number]) {
    assertFinite(values)
    this.values = [...values]
  }

  static fromMatrix<From extends CoordinateSpace, To extends CoordinateSpace>(
    values: readonly [number, number, number, number, number, number],
  ): Transform2D<From, To> {
    return new Transform2D(values)
  }

  static scale<From extends CoordinateSpace, To extends CoordinateSpace>(
    x: number,
    y: number,
  ): Transform2D<From, To> {
    return Transform2D.fromMatrix([x, 0, 0, y, 0, 0])
  }

  apply(value: Point<From>): Point<To> {
    const [a, b, c, d, tx, ty] = this.values
    return point<To>(a * value.x + c * value.y + tx, b * value.x + d * value.y + ty)
  }

  applyVector(value: Point<From>): Point<To> {
    const [a, b, c, d] = this.values
    return point<To>(a * value.x + c * value.y, b * value.x + d * value.y)
  }

  applyRect(value: Rect<From>): Rect<To> {
    const corners = [
      this.apply(point<From>(value.x, value.y)),
      this.apply(point<From>(value.x + value.width, value.y)),
      this.apply(point<From>(value.x, value.y + value.height)),
      this.apply(point<From>(value.x + value.width, value.y + value.height)),
    ]
    const xs = corners.map((corner) => corner.x)
    const ys = corners.map((corner) => corner.y)
    const left = Math.min(...xs)
    const top = Math.min(...ys)
    return rect<To>(left, top, Math.max(...xs) - left, Math.max(...ys) - top)
  }

  inverse(): Transform2D<To, From> {
    const [a, b, c, d, tx, ty] = this.values
    const determinant = a * d - b * c
    if (!Number.isFinite(determinant) || Math.abs(determinant) <= 1e-12) {
      throw new GeometryError('Transform is singular')
    }
    return Transform2D.fromMatrix([
      d / determinant,
      -b / determinant,
      -c / determinant,
      a / determinant,
      (c * ty - d * tx) / determinant,
      (b * tx - a * ty) / determinant,
    ])
  }

  then<Next extends CoordinateSpace>(next: Transform2D<To, Next>): Transform2D<From, Next> {
    const [a, b, c, d, tx, ty] = this.values
    const [na, nb, nc, nd, ntx, nty] = next.values
    return Transform2D.fromMatrix([
      na * a + nc * b,
      nb * a + nd * b,
      na * c + nc * d,
      nb * c + nd * d,
      na * tx + nc * ty + ntx,
      nb * tx + nd * ty + nty,
    ])
  }
}

export function clampRect<S extends CoordinateSpace>(
  value: Rect<S>,
  bounds: Rect<S>,
  minimum: readonly [number, number] = [0, 0],
): Rect<S> {
  const minWidth = Math.max(0, Math.min(minimum[0], bounds.width))
  const minHeight = Math.max(0, Math.min(minimum[1], bounds.height))
  const width = Math.max(minWidth, Math.min(value.width, bounds.width))
  const height = Math.max(minHeight, Math.min(value.height, bounds.height))
  const x = Math.max(bounds.x, Math.min(value.x, bounds.x + bounds.width - width))
  const y = Math.max(bounds.y, Math.min(value.y, bounds.y + bounds.height - height))
  return rect<S>(x, y, width, height)
}

export class GeometryError extends Error {}

function assertFinite(values: readonly number[]): void {
  if (!values.every(Number.isFinite)) throw new GeometryError('Geometry values must be finite')
}
