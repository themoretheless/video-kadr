import { describe, expect, it } from 'vitest'
import type { Capabilities } from '$lib/types'
import {
  parseProxyPreferences,
  proxyCodecOptions,
  resolveProxyPlayback,
  serializeProxyPreferences,
} from './model'
import type { ProxyList } from './types'

const key = 'a'.repeat(64)
const list: ProxyList = {
  sourceId: 'source-one',
  sourceFingerprint: 'b'.repeat(64),
  status: 'ready',
  proxies: [{
    key,
    profile: { maxWidth: 720, codec: 'h264', quality: 28, includeAudio: true },
    status: 'ready',
    url: `/files/proxies/${key}.mp4`,
    sizeBytes: 20,
    sha256: 'c'.repeat(64),
  }],
  jobs: [],
}

const capabilities: Capabilities = {
  schemaVersion: 1,
  toolFingerprint: 'test',
  formats: [
    { id: 'mp4', label: 'MP4', available: true },
    { id: 'prores', label: 'ProRes', available: true },
  ],
  codecs: [{ id: 'h264', label: 'H.264', available: true }],
  filters: [],
  hardware: [],
}

describe('proxy preview model', () => {
  it('persists only bounded explicit original/proxy preferences', () => {
    const preferences = { source: { mode: 'proxy' as const, key }, other: { mode: 'original' as const } }
    expect(parseProxyPreferences(serializeProxyPreferences(preferences))).toEqual(preferences)
    expect(parseProxyPreferences('{"source":{"mode":"proxy","key":"../escape"}}')).toEqual({})
    expect(parseProxyPreferences('{"source":{"mode":"original","extra":true}}')).toEqual({})
  })

  it('uses a ready proxy but falls back to the original when stale or playback failed', () => {
    const selected = resolveProxyPlayback('/files/sources/original.mp4', list, { mode: 'proxy', key })
    expect(selected).toMatchObject({ kind: 'proxy', url: `/files/proxies/${key}.mp4` })

    expect(resolveProxyPlayback('/files/sources/original.mp4', list, { mode: 'proxy', key: 'd'.repeat(64) }))
      .toMatchObject({ kind: 'original', fallbackReason: expect.stringContaining('устарел') })
    expect(resolveProxyPlayback('/files/sources/original.mp4', list, { mode: 'proxy', key }, key))
      .toMatchObject({ kind: 'original', fallbackReason: expect.stringContaining('воспроизвести') })

    const prores: ProxyList = {
      ...list,
      proxies: [{
        ...list.proxies[0]!,
        profile: { ...list.proxies[0]!.profile, codec: 'prores_proxy' },
        url: `/files/proxies/${key}.mov`,
      }],
    }
    expect(resolveProxyPlayback('/files/sources/original.mov', prores, { mode: 'proxy', key }, null, 'other'))
      .toMatchObject({ kind: 'original', fallbackReason: expect.stringContaining('платформе') })
    expect(resolveProxyPlayback('/files/sources/original.mov', prores, { mode: 'proxy', key }, null, 'mac').kind)
      .toBe('proxy')
  })

  it('gates H.264 by server capability and ProRes Proxy by capability plus platform', () => {
    expect(proxyCodecOptions(capabilities, 'other')).toMatchObject([
      { codec: 'h264', available: true },
      { codec: 'prores_proxy', available: false },
    ])
    expect(proxyCodecOptions(capabilities, 'mac')[1]).toMatchObject({ codec: 'prores_proxy', available: true })
    expect(proxyCodecOptions(null, 'mac').every(({ available }) => !available)).toBe(true)
  })
})
