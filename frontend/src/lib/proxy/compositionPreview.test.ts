import { describe, expect, it } from 'vitest'
import {
  compositionPreviewResolutionOptions,
  isActiveCompositionVideoSource,
  resolveCompositionPreviewMedia,
  type CompositionPreviewSourceRequest,
} from './compositionPreview'
import type { ProxyArtifact, ProxyList } from './types'

const audioKey = 'a'.repeat(64)
const silentKey = 'b'.repeat(64)
const proresKey = 'c'.repeat(64)

function artifact(
  key: string,
  maxWidth: number,
  includeAudio = true,
  codec: 'h264' | 'prores_proxy' = 'h264',
): ProxyArtifact {
  return {
    key,
    profile: { maxWidth, codec, quality: codec === 'h264' ? 28 : 0, includeAudio },
    status: 'ready',
    url: `/files/proxies/${key}.${codec === 'h264' ? 'mp4' : 'mov'}`,
    sizeBytes: 1024,
    sha256: 'd'.repeat(64),
  }
}

function proxyList(proxies: ProxyArtifact[]): ProxyList {
  return {
    sourceId: 'video-one',
    sourceFingerprint: 'e'.repeat(64),
    status: proxies.length ? 'ready' : 'none',
    proxies,
    jobs: [],
  }
}

const source: CompositionPreviewSourceRequest = {
  sourceId: 'video-one',
  kind: 'video',
  active: true,
  originalUrl: '/files/sources/original.mp4',
  audibleSourceAudio: false,
}

describe('composition proxy preview resolver', () => {
  it('accepts only safe active video sources with an original URL', () => {
    expect(isActiveCompositionVideoSource(source)).toBe(true)
    expect(isActiveCompositionVideoSource({ ...source, kind: 'image' })).toBe(false)
    expect(isActiveCompositionVideoSource({ ...source, active: false })).toBe(false)
    expect(isActiveCompositionVideoSource({ ...source, sourceId: '../escape' })).toBe(false)
    expect(isActiveCompositionVideoSource({ ...source, originalUrl: '' })).toBe(false)
  })

  it('honors each source preference without changing original identity', () => {
    const list = proxyList([artifact(audioKey, 720)])
    expect(resolveCompositionPreviewMedia(source, list, { mode: 'proxy', key: audioKey }, null, 'other'))
      .toMatchObject({
        kind: 'proxy',
        url: `/files/proxies/${audioKey}.mp4`,
      })
    expect(resolveCompositionPreviewMedia(source, list, { mode: 'original' }, null, 'other'))
      .toMatchObject({
        kind: 'original',
        url: source.originalUrl,
      })
  })

  it('falls back to original for missing, stale, unsafe, unsupported or failed proxies', () => {
    const list = proxyList([artifact(audioKey, 720), artifact(proresKey, 960, true, 'prores_proxy')])
    const missing = resolveCompositionPreviewMedia(source, null, { mode: 'proxy', key: audioKey }, null, 'other')
    expect(missing).toMatchObject({ kind: 'original', url: source.originalUrl })
    expect(missing?.fallbackReason).toContain('Проверяем')

    const stale = resolveCompositionPreviewMedia(source, list, { mode: 'proxy', key: silentKey }, null, 'other')
    expect(stale?.fallbackReason).toContain('устарел')

    const unsafe = proxyList([{ ...artifact(audioKey, 720), url: 'https://example.test/proxy.mp4' }])
    expect(resolveCompositionPreviewMedia(source, unsafe, { mode: 'proxy', key: audioKey }, null, 'other'))
      .toMatchObject({ kind: 'original', url: source.originalUrl })

    expect(resolveCompositionPreviewMedia(source, list, { mode: 'proxy', key: proresKey }, null, 'other')?.fallbackReason)
      .toContain('платформе')
    expect(resolveCompositionPreviewMedia(source, list, { mode: 'proxy', key: audioKey }, audioKey, 'other')?.fallbackReason)
      .toContain('воспроизвести')

    const mismatched = { ...list, sourceId: 'different-source' }
    expect(resolveCompositionPreviewMedia(source, mismatched, { mode: 'proxy', key: audioKey }, null, 'other'))
      .toMatchObject({ kind: 'original', url: source.originalUrl })
  })

  it('uses original when audible embedded audio needs a proxy that has no audio', () => {
    const list = proxyList([
      artifact(silentKey, 480, false),
      artifact(audioKey, 720, true),
    ])
    const audible = { ...source, audibleSourceAudio: true }
    const fallback = resolveCompositionPreviewMedia(
      audible,
      list,
      { mode: 'proxy', key: silentKey },
      null,
      'other',
    )
    expect(fallback).toMatchObject({
      kind: 'original',
      url: source.originalUrl,
    })
    expect(fallback?.fallbackReason).toContain('без звука')

    expect(resolveCompositionPreviewMedia(source, list, { mode: 'proxy', key: silentKey }, null, 'other'))
      .toMatchObject({ kind: 'proxy', artifact: { key: silentKey } })
  })

  it('offers only real ready proxies that can satisfy current playback', () => {
    const list = proxyList([
      artifact(audioKey, 720, true),
      artifact(silentKey, 480, false),
      artifact(proresKey, 960, true, 'prores_proxy'),
    ])
    expect(compositionPreviewResolutionOptions(
      { ...source, audibleSourceAudio: true },
      list,
      null,
      'other',
    ).map(({ value, label }) => ({ value, label }))).toEqual([
      { value: 'original', label: 'Original · исходное разрешение' },
      { value: audioKey, label: 'Proxy · 720px · H.264 · со звуком' },
    ])
    expect(compositionPreviewResolutionOptions(source, list, audioKey, 'mac').map(({ value }) => value))
      .toEqual(['original', silentKey, proresKey])
  })
})
