export const EXPORT_BATCH_CONTRACT = 'export-batch-v1' as const
export const MAX_EXPORT_VARIANTS = 32
export const MAX_EXPORT_PAYLOAD_BYTES = 256 * 1024
export const MAX_EXPORT_DEPENDENCIES = 64

export type ExportDependencyKind = 'source' | 'lut'
export interface ExportDependency {
  kind: ExportDependencyKind
  assetRef: string
  fingerprint: string
}

export interface ExportVariantInput {
  id: string
  label: string
  overrides: Record<string, unknown>
}

export interface ExportBatchInput {
  id: string
  source: { assetRef: string; fingerprint: string }
  dependencies?: ExportDependency[]
  basePayload: Record<string, unknown>
  variants: ExportVariantInput[]
}

export interface ExportJobDefinitionV1 {
  contract: typeof EXPORT_BATCH_CONTRACT
  id: string
  batchId: string
  variantId: string
  ordinal: number
  label: string
  source: { assetRef: string; fingerprint: string }
  dependencies: ExportDependency[]
  payload: Record<string, unknown>
}

const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const SHA = /^[a-f0-9]{64}$/

function validateSerializable(value: unknown, seen = new Set<object>()): void {
  if (value === undefined || value === null || ['string', 'boolean'].includes(typeof value)) return
  if (typeof value === 'number') { if (!Number.isFinite(value)) throw new Error('export payload contains non-finite number'); return }
  if (typeof value !== 'object') throw new Error('export payload is not serializable')
  if (seen.has(value as object)) throw new Error('export payload contains a cycle')
  if (value instanceof Blob || value instanceof ArrayBuffer || ArrayBuffer.isView(value)) throw new Error('export payload must not contain binary data')
  const prototype = Object.getPrototypeOf(value)
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) throw new Error('export payload contains unsupported object')
  seen.add(value as object)
  for (const child of Array.isArray(value) ? value : Object.values(value as Record<string, unknown>)) validateSerializable(child, seen)
  seen.delete(value as object)
}

function canonical(value: unknown): unknown {
  if (value === undefined) return null
  if (Array.isArray(value)) return value.map(canonical)
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value as Record<string, unknown>)
    .filter(key => (value as Record<string, unknown>)[key] !== undefined)
    .sort().map(key => [key, canonical((value as Record<string, unknown>)[key])]))
  return value
}

function snapshot(value: Record<string, unknown>): Record<string, unknown> {
  validateSerializable(value)
  const result = canonical(structuredClone(value)) as Record<string, unknown>
  if (new TextEncoder().encode(JSON.stringify(result)).byteLength > MAX_EXPORT_PAYLOAD_BYTES) throw new Error('export payload exceeds size limit')
  return result
}

function dependencyManifest(source: ExportBatchInput['source'], dependencies: readonly ExportDependency[] = []): ExportDependency[] {
  const values: ExportDependency[] = [{ kind: 'source', ...source }, ...dependencies.map(item => ({ ...item }))]
  if (values.length > MAX_EXPORT_DEPENDENCIES) throw new Error('export dependency manifest exceeds limit')
  const unique = new Map<string, ExportDependency>()
  for (const dependency of values) {
    if (!['source', 'lut'].includes(dependency.kind) || !ID.test(dependency.assetRef) || !SHA.test(dependency.fingerprint)) throw new Error('invalid export dependency')
    const key = `${dependency.kind}:${dependency.assetRef}`
    const existing = unique.get(key)
    if (existing && existing.fingerprint !== dependency.fingerprint) throw new Error('conflicting export dependency')
    unique.set(key, { ...dependency })
  }
  return [...unique.values()].sort((a, b) => a.kind.localeCompare(b.kind) || a.assetRef.localeCompare(b.assetRef))
}

export function expandExportBatch(input: ExportBatchInput): ExportJobDefinitionV1[] {
  if (!ID.test(input.id) || !ID.test(input.source.assetRef) || !SHA.test(input.source.fingerprint)) throw new Error('invalid export batch identity')
  if (!Array.isArray(input.variants) || input.variants.length < 1 || input.variants.length > MAX_EXPORT_VARIANTS) throw new Error('invalid export variant count')
  const ids = new Set<string>()
  const dependencies = dependencyManifest(input.source, input.dependencies)
  return input.variants.map((variant, ordinal) => {
    if (!ID.test(variant.id) || ids.has(variant.id) || typeof variant.label !== 'string' || !variant.label.trim() || variant.label.length > 120) throw new Error('invalid export variant')
    ids.add(variant.id)
    const payload = snapshot({ ...snapshot(input.basePayload), ...snapshot(variant.overrides) })
    return {
      contract: EXPORT_BATCH_CONTRACT, id: `${input.id}:${variant.id}`, batchId: input.id,
      variantId: variant.id, ordinal, label: variant.label.trim(), source: { ...input.source }, dependencies: structuredClone(dependencies), payload,
    }
  })
}

export function canonicalExportDefinition(value: ExportJobDefinitionV1): string {
  return JSON.stringify(canonical(value))
}

export function validateExportJobDefinition(value: ExportJobDefinitionV1): ExportJobDefinitionV1 {
  if (value.contract !== EXPORT_BATCH_CONTRACT || !ID.test(value.id) || !ID.test(value.batchId)
    || !ID.test(value.variantId) || value.id !== `${value.batchId}:${value.variantId}`
    || !Number.isSafeInteger(value.ordinal) || value.ordinal < 0 || value.ordinal >= MAX_EXPORT_VARIANTS
    || typeof value.label !== 'string' || !value.label.trim() || value.label.length > 120
    || !ID.test(value.source?.assetRef) || !SHA.test(value.source?.fingerprint)) throw new Error('invalid export job definition')
  const dependencies = dependencyManifest(value.source, value.dependencies)
  if (dependencies.length !== value.dependencies?.length) throw new Error('duplicate export dependency')
  return { ...structuredClone(value), label: value.label.trim(), payload: snapshot(value.payload), source: { ...value.source }, dependencies }
}
