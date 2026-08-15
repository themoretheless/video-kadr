import { describe, expect, it } from 'vitest'
import {
  isSafeProxyMediaUrl,
  parseProxyCreateResult,
  parseProxyList,
  parseProxyProfile,
} from './types'

const key = 'a'.repeat(64)
const sha256 = 'b'.repeat(64)
const sourceFingerprint = 'c'.repeat(64)
const profile = { maxWidth: 960, codec: 'h264' as const, quality: 28, includeAudio: true }

function readyList() {
  return {
    sourceId: 'source-one',
    sourceFingerprint,
    status: 'ready',
    proxies: [{
      key,
      profile,
      status: 'ready',
      url: `/files/proxies/${key}.mp4`,
      sizeBytes: 1024,
      sha256,
    }],
    jobs: [],
  }
}

describe('strict proxy wire types', () => {
  it('accepts the bounded canonical profile and response shapes', () => {
    expect(parseProxyProfile(profile)).toEqual(profile)
    expect(parseProxyCreateResult({ jobId: 'job-1', key })).toEqual({ jobId: 'job-1', key })
    expect(parseProxyList(readyList(), 'source-one')).toEqual(readyList())
  })

  it('rejects unknown fields, inconsistent statuses and duplicate artifacts', () => {
    expect(() => parseProxyProfile({ ...profile, preset: 'fast' })).toThrow('profile')
    expect(() => parseProxyList({ ...readyList(), status: 'none' })).toThrow('list.status')
    const list = readyList()
    expect(() => parseProxyList({ ...list, proxies: [list.proxies[0], list.proxies[0]] })).toThrow('duplicates')
  })

  it('allows only exact same-origin proxy media routes with matching extension and key', () => {
    expect(isSafeProxyMediaUrl(`/files/proxies/${key}.mp4`, key, 'h264')).toBe(true)
    expect(isSafeProxyMediaUrl(`//evil.example/files/proxies/${key}.mp4`, key, 'h264')).toBe(false)
    expect(isSafeProxyMediaUrl(`/files/proxies/${key}.mov`, key, 'h264')).toBe(false)
    expect(isSafeProxyMediaUrl(`/files/proxies/${key}.mp4?download=1`, key, 'h264')).toBe(false)
    expect(() => parseProxyList({
      ...readyList(),
      proxies: [{ ...readyList().proxies[0], url: `https://evil.example/${key}.mp4` }],
    })).toThrow('artifact.url')
  })
})
