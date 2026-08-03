import { describe, expect, it } from 'vitest'
import { canonicalExportDefinition, expandExportBatch, MAX_EXPORT_VARIANTS, validateExportJobDefinition } from './export-variants'

const source = { assetRef: 'asset-1', fingerprint: 'a'.repeat(64) }

describe('export batch variants v1', () => {
  it('expands immutable canonical snapshots in declared order', () => {
    const base = { quality: 20, nested: { z: 1, a: 2 } }
    const jobs = expandExportBatch({ id: 'batch-1', source, basePayload: base, variants: [
      { id: 'web', label: 'Web', overrides: { format: 'webm' } },
      { id: 'master', label: 'Master', overrides: { format: 'mp4', quality: 12 } },
    ] })
    base.nested.z = 99
    expect(jobs.map(job => [job.id, job.ordinal, job.payload.quality])).toEqual([['batch-1:web', 0, 20], ['batch-1:master', 1, 12]])
    expect(canonicalExportDefinition(jobs[0]!)).toContain('"a":2')
  })

  it('rejects duplicate/excess variants and unsafe payloads', () => {
    expect(() => expandExportBatch({ id: 'b', source, basePayload: {}, variants: Array.from({ length: MAX_EXPORT_VARIANTS + 1 }, (_, id) => ({ id: `v${id}`, label: 'x', overrides: {} })) })).toThrow('count')
    expect(() => expandExportBatch({ id: 'b', source, basePayload: {}, variants: [{ id: 'v', label: 'x', overrides: {} }, { id: 'v', label: 'y', overrides: {} }] })).toThrow('variant')
    expect(() => expandExportBatch({ id: 'b', source, basePayload: { file: new Blob(['x']) }, variants: [{ id: 'v', label: 'x', overrides: {} }] })).toThrow('binary')
  })

  it('omits optional undefined object fields and normalizes array holes deterministically', () => {
    const [definition] = expandExportBatch({
      id: 'optional', source,
      basePayload: { format: 'mp4', codec: undefined, nested: { present: 1, absent: undefined }, values: ['a', undefined] },
      variants: [{ id: 'default', label: 'Default', overrides: { quality: undefined } }],
    })
    expect(definition!.payload).toEqual({ format: 'mp4', nested: { present: 1 }, values: ['a', null] })
    const before = canonicalExportDefinition(definition!)
    const persisted = validateExportJobDefinition(structuredClone(definition!))
    expect(persisted.payload).toEqual(definition!.payload)
    expect(canonicalExportDefinition(persisted)).toBe(before)
  })

  it('persists a canonical complete dependency manifest and rejects conflicts', () => {
    const angle = { kind: 'source' as const, assetRef: 'angle-b', fingerprint: 'b'.repeat(64) }
    const lut = { kind: 'lut' as const, assetRef: 'look-1', fingerprint: 'c'.repeat(64) }
    const [definition] = expandExportBatch({ id: 'deps', source, dependencies: [lut, angle, angle], basePayload: {}, variants: [{ id: 'v', label: 'V', overrides: {} }] })
    expect(definition!.dependencies).toEqual([
      lut,
      angle,
      { kind: 'source', ...source },
    ])
    expect(validateExportJobDefinition(structuredClone(definition!)).dependencies).toEqual(definition!.dependencies)
    expect(() => expandExportBatch({ id: 'conflict', source, dependencies: [{ ...angle }, { ...angle, fingerprint: 'd'.repeat(64) }], basePayload: {}, variants: [{ id: 'v', label: 'V', overrides: {} }] })).toThrow('conflicting')
  })
})
