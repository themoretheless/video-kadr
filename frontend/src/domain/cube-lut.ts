import { sha256 } from '@noble/hashes/sha2.js'

export const CUBE_LUT_CONTRACT = 'cube-lut-canonical-v1' as const
export const MAX_CUBE_LUT_BYTES = 16 * 1024 * 1024
export const MAX_CUBE_LUT_SIZE = 65
const MAX_LINE_BYTES = 4096
const MAX_TITLE_CHARS = 128

export type RgbTuple = readonly [number, number, number]

export class CubeLutError extends Error {
  constructor(public readonly code: string) {
    super(`Invalid 3D CUBE LUT: ${code}`)
    this.name = 'CubeLutError'
  }
}

export interface ParsedCubeLut {
  contract: typeof CUBE_LUT_CONTRACT
  cubeSize: number
  domainMin: RgbTuple
  domainMax: RgbTuple
  /** RGB triplets in CUBE order: red changes fastest, then green, then blue. */
  values: Float64Array
  canonicalText: string
  canonicalBytes: Uint8Array
  sha256: string
}

const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true })

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, value => value.toString(16).padStart(2, '0')).join('')
}

export function sha256Hex(bytes: Uint8Array): string {
  return hex(sha256(bytes))
}

function parseNumber(token: string, code: string): number {
  // Number() accepts empty strings and hexadecimal syntax; CUBE does not.
  if (/^[+-]?(?:nan|inf|infinity)$/i.test(token)) throw new CubeLutError('non_finite_number')
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(token)) {
    throw new CubeLutError(code)
  }
  const value = Number(token)
  if (!Number.isFinite(value)) throw new CubeLutError('non_finite_number')
  return Object.is(value, -0) ? 0 : value
}

function triplet(rest: string, code = 'invalid_triplet'): RgbTuple {
  const tokens = rest.trim().split(/\s+/)
  if (tokens.length !== 3 || tokens.some(token => token === '')) throw new CubeLutError(code)
  return [
    parseNumber(tokens[0]!, code),
    parseNumber(tokens[1]!, code),
    parseNumber(tokens[2]!, code),
  ]
}

/** Stable shortest decimal representation used for browser content identity. */
export function canonicalCubeNumber(value: number): string {
  if (!Number.isFinite(value)) throw new CubeLutError('non_finite_number')
  if (value === 0) return '0'
  const raw = value.toString()
  if (!/[eE]/.test(raw)) return raw
  // Rust f64 Display expands scientific notation. Do the same so browser and
  // server canonical uploads have one content SHA even at exponent thresholds.
  const [coefficient, exponentToken] = raw.toLowerCase().split('e') as [string, string]
  const exponent = Number(exponentToken)
  const negative = coefficient.startsWith('-')
  const unsigned = negative ? coefficient.slice(1) : coefficient
  const [whole, fraction = ''] = unsigned.split('.')
  const digits = `${whole}${fraction}`
  const decimal = whole.length + exponent
  const expanded = decimal <= 0
    ? `0.${'0'.repeat(-decimal)}${digits}`
    : decimal >= digits.length
      ? `${digits}${'0'.repeat(decimal - digits.length)}`
      : `${digits.slice(0, decimal)}.${digits.slice(decimal)}`
  return negative ? `-${expanded}` : expanded
}

function canonicalTriplet(value: RgbTuple): string {
  return value.map(canonicalCubeNumber).join(' ')
}

export function encodeCanonicalCube(
  size: number,
  values: Iterable<RgbTuple>,
  domainMin: RgbTuple = [0, 0, 0],
  domainMax: RgbTuple = [1, 1, 1],
): { text: string; bytes: Uint8Array; sha256: string } {
  if (!Number.isInteger(size) || size < 2 || size > MAX_CUBE_LUT_SIZE) {
    throw new CubeLutError('invalid_size')
  }
  if (!domainMin.every((value, index) => Number.isFinite(value) && value < domainMax[index]!)) {
    throw new CubeLutError('invalid_domain')
  }
  const expected = size ** 3
  const lines = [
    `LUT_3D_SIZE ${size}`,
    `DOMAIN_MIN ${canonicalTriplet(domainMin)}`,
    `DOMAIN_MAX ${canonicalTriplet(domainMax)}`,
  ]
  let count = 0
  for (const value of values) {
    if (count >= expected) throw new CubeLutError('wrong_entry_count')
    lines.push(canonicalTriplet(value))
    count++
  }
  if (count !== expected) throw new CubeLutError('wrong_entry_count')
  const text = `${lines.join('\n')}\n`
  const bytes = encoder.encode(text)
  if (bytes.byteLength > MAX_CUBE_LUT_BYTES) throw new CubeLutError('too_large')
  return { text, bytes, sha256: sha256Hex(bytes) }
}

/** Strict, bounded parser matching the canonical server-supported 3D subset. */
export function parseCubeLut(input: string | Uint8Array | ArrayBuffer): ParsedCubeLut {
  const bytes = typeof input === 'string'
    ? encoder.encode(input)
    : input instanceof Uint8Array ? input : new Uint8Array(input)
  if (bytes.byteLength > MAX_CUBE_LUT_BYTES) throw new CubeLutError('too_large')
  let text: string
  try {
    text = decoder.decode(bytes)
  } catch {
    throw new CubeLutError('invalid_utf8')
  }
  if (text.includes('\0')) throw new CubeLutError('invalid_utf8')
  if (text.startsWith('\uFEFF')) text = text.slice(1)

  let size: number | null = null
  let titleSeen = false
  let domainMin: RgbTuple | null = null
  let domainMax: RgbTuple | null = null
  let dataStarted = false
  const values: number[] = []
  for (const raw of text.split(/\r?\n/)) {
    if (encoder.encode(raw).byteLength > MAX_LINE_BYTES) throw new CubeLutError('line_too_long')
    const line = raw.trim()
    if (!line || line.startsWith('#')) continue
    const keyword = line.split(/\s+/, 1)[0]!
    const rejectLate = () => { if (dataStarted) throw new CubeLutError('header_after_data') }
    if (keyword === 'TITLE') {
      rejectLate()
      if (titleSeen) throw new CubeLutError('duplicate_header')
      const value = line.slice(5).trim()
      if (value.length < 2 || !value.startsWith('"') || !value.endsWith('"')) throw new CubeLutError('invalid_title')
      const title = value.slice(1, -1)
      if ([...title].length > MAX_TITLE_CHARS || /[\p{Cc}"]/u.test(title)) throw new CubeLutError('invalid_title')
      titleSeen = true
      continue
    }
    if (keyword === 'LUT_1D_SIZE') throw new CubeLutError('unsupported_one_dimensional')
    if (keyword === 'LUT_3D_SIZE') {
      rejectLate()
      if (size !== null) throw new CubeLutError('duplicate_header')
      const token = line.slice(11).trim()
      if (!/^\d+$/.test(token)) throw new CubeLutError('invalid_size')
      size = Number(token)
      if (!Number.isInteger(size) || size < 2 || size > MAX_CUBE_LUT_SIZE) throw new CubeLutError('invalid_size')
      continue
    }
    if (keyword === 'DOMAIN_MIN' || keyword === 'DOMAIN_MAX') {
      rejectLate()
      if (keyword === 'DOMAIN_MIN') {
        if (domainMin) throw new CubeLutError('duplicate_header')
        domainMin = triplet(line.slice(10), 'invalid_domain')
      } else {
        if (domainMax) throw new CubeLutError('duplicate_header')
        domainMax = triplet(line.slice(10), 'invalid_domain')
      }
      continue
    }
    if (/^[A-Za-z_]/.test(keyword) && !/^(?:nan|inf|infinity)$/i.test(keyword)) throw new CubeLutError('unknown_header')
    if (size === null) throw new CubeLutError('missing_size')
    dataStarted = true
    if (values.length / 3 >= size ** 3) throw new CubeLutError('wrong_entry_count')
    values.push(...triplet(line))
  }
  if (size === null) throw new CubeLutError('missing_size')
  if (values.length !== size ** 3 * 3) throw new CubeLutError('wrong_entry_count')
  const min = domainMin ?? [0, 0, 0]
  const max = domainMax ?? [1, 1, 1]
  if (!min.every((value, index) => value < max[index]!)) throw new CubeLutError('invalid_domain')
  const tuples = function* (): Iterable<RgbTuple> {
    for (let index = 0; index < values.length; index += 3) {
      yield [values[index]!, values[index + 1]!, values[index + 2]!]
    }
  }
  const canonical = encodeCanonicalCube(size, tuples(), min, max)
  return {
    contract: CUBE_LUT_CONTRACT,
    cubeSize: size,
    domainMin: min,
    domainMax: max,
    values: Float64Array.from(values),
    canonicalText: canonical.text,
    canonicalBytes: canonical.bytes,
    sha256: canonical.sha256,
  }
}
