import { describe, expect, it } from 'vitest'
import { migrateBrandKit, nextBrandKitRevision, type BrandKit } from './brand-kit'

const kit = (): BrandKit => ({
  schemaVersion: 1, id: 'kit-1', revision: 1, name: 'Acme',
  colors: [{ id: 'color-primary', name: 'Primary', value: { space: 'srgb', rgba: [1, 0.5, 0, 1] } }],
  fonts: [{ id: 'font-body', name: 'Body', family: 'Inter', weight: 400, style: 'normal', asset: { assetRef: 'font-file', fingerprint: 'a'.repeat(64), mimeType: 'font/woff2', byteLength: 123 } }],
  logos: [{ id: 'logo-main', name: 'Main', variant: 'primary', asset: { assetRef: 'logo-file', fingerprint: 'b'.repeat(64), mimeType: 'image/png', byteLength: 456 } }],
  defaults: { primaryColorId: 'color-primary', bodyFontId: 'font-body', primaryLogoId: 'logo-main' },
})

describe('brand kit domain', () => {
  it('decodes an isolated canonical copy and advances immutable revisions', () => {
    const source = kit(); const decoded = migrateBrandKit(source); decoded.name = 'Changed'
    expect(source.name).toBe('Acme')
    const next = nextBrandKitRevision(source, { ...source, name: 'Acme 2', colors: source.colors, fonts: source.fonts, logos: source.logos, defaults: source.defaults } as never)
    expect(next).toMatchObject({ id: 'kit-1', revision: 2, name: 'Acme 2' }); expect(source.revision).toBe(1)
  })
  it('rejects unknown fields, unsafe assets and dangling defaults', () => {
    expect(() => migrateBrandKit({ ...kit(), injected: true })).toThrow('unexpected injected')
    expect(() => migrateBrandKit({ ...kit(), fonts: [{ ...kit().fonts[0], asset: { ...kit().fonts[0]!.asset, mimeType: 'font/ttf' } }] })).toThrow('invalid brand font asset')
    expect(() => migrateBrandKit({ ...kit(), defaults: { primaryColorId: 'missing' } })).toThrow('invalid brand default')
  })
  it('rejects non-canonical colors and future versions', () => {
    const invalid = kit(); invalid.colors[0]!.value.rgba[0] = -0
    expect(() => migrateBrandKit(invalid)).toThrow('invalid brand color value')
    expect(() => migrateBrandKit({ schemaVersion: 2 })).toThrow('unsupported brand kit schemaVersion 2')
  })
})
