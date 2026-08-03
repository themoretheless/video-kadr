import { describe, expect, it } from 'vitest'
import {
  defaultProjectArchiveAction,
  defaultProjectArchiveMediaAction,
  planProjectArchiveExport,
  type ProjectArchiveAssetCandidate,
} from './project-archive-contract'

const assets: ProjectArchiveAssetCandidate[] = [
  { id: 'source-a', role: 'original', displayName: 'A.mov', sizeBytes: 100, availability: 'ready' },
  { id: 'source-b', role: 'original', displayName: 'B.wav', sizeBytes: 40, availability: 'ready' },
  { id: 'proxy-a', role: 'proxy', displayName: 'A proxy', sizeBytes: 10, availability: 'ready' },
  { id: 'lut-a', role: 'lut', displayName: 'Look.cube', sizeBytes: 5, availability: 'ready', requiredForEdit: true },
]

describe('portable project archive contract', () => {
  it('always carries non-rebuildable edit dependencies and includes selected bytes', () => {
    const projectOnly = planProjectArchiveExport(20, {
      includeOriginalMedia: false,
      includeProxies: false,
    }, assets)
    expect(projectOnly.includedAssetIds).toEqual(['lut-a'])
    expect(projectOnly.estimatedBytes).toBe(25)
    expect(projectOnly.warnings.filter(issue => issue.code === 'relink_required')).toHaveLength(2)

    const complete = planProjectArchiveExport(20, {
      includeOriginalMedia: true,
      includeProxies: true,
    }, assets)
    expect(complete.includedAssetIds).toEqual(['source-a', 'source-b', 'proxy-a', 'lut-a'])
    expect(complete.estimatedBytes).toBe(175)
    expect(complete.blockers).toEqual([])
  })

  it('blocks a promised original or required LUT that cannot be read, but skips a rebuildable proxy', () => {
    const plan = planProjectArchiveExport(20, {
      includeOriginalMedia: true,
      includeProxies: true,
    }, [
      { id: 'source', role: 'original', displayName: 'Gone.mov', sizeBytes: 100, availability: 'offline' },
      { id: 'proxy', role: 'proxy', displayName: 'Gone proxy', sizeBytes: 10, availability: 'offline' },
      { id: 'lut', role: 'lut', displayName: 'Gone.cube', sizeBytes: 5, availability: 'permission-required', requiredForEdit: true },
    ])
    expect(plan.blockers.map(issue => issue.code)).toEqual(['missing_original', 'missing_required_asset'])
    expect(plan.warnings.map(issue => issue.code)).toContain('proxy_skipped')
    expect(plan.includedAssetIds).toEqual([])
  })

  it('makes proxy-only portability limitations explicit', () => {
    const plan = planProjectArchiveExport(20, {
      includeOriginalMedia: false,
      includeProxies: true,
    }, assets)
    expect(plan.warnings.map(issue => issue.code)).toContain('proxy_only')
  })

  it('never defaults to destructive project or media replacement', () => {
    expect(defaultProjectArchiveAction('absent')).toBe('create')
    expect(defaultProjectArchiveAction('identical')).toBe('reuse')
    expect(defaultProjectArchiveAction('diverged')).toBe('copy')
    expect(defaultProjectArchiveMediaAction('absent')).toBe('create')
    expect(defaultProjectArchiveMediaAction('same-fingerprint')).toBe('reuse')
    expect(defaultProjectArchiveMediaAction('different-fingerprint')).toBe('rekey')
  })

  it('fails closed on duplicate or invalid inventory identities', () => {
    const plan = planProjectArchiveExport(1, {
      includeOriginalMedia: true,
      includeProxies: false,
    }, [
      { id: 'same', role: 'original', displayName: 'A.mov', sizeBytes: 1, availability: 'ready' },
      { id: 'same', role: 'original', displayName: 'B.mov', sizeBytes: 1, availability: 'ready' },
    ])
    expect(plan.blockers.map(issue => issue.code)).toEqual(['invalid_asset'])
  })

  it('accepts readable session media but rejects zero-byte payloads like the Rust contract', () => {
    const plan = planProjectArchiveExport(1, {
      includeOriginalMedia: true,
      includeProxies: false,
    }, [
      { id: 'session', role: 'original', displayName: 'Session.mov', sizeBytes: 2, availability: 'session' },
      { id: 'empty', role: 'original', displayName: 'Empty.mov', sizeBytes: 0, availability: 'ready' },
    ])
    expect(plan.includedAssetIds).toEqual(['session'])
    expect(plan.blockers.map(issue => issue.code)).toEqual(['invalid_asset'])
  })
})
