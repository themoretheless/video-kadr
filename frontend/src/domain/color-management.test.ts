import { describe, expect, it } from 'vitest'
import { BROWSER_DECODED_SRGB_STATUS, FIXED_VIDEO_OUTPUT_REC709_LIMITED, outputColorStatus, parseColorManagementStatusV1, sourceColorStatus } from './color-management'

describe('backend-compatible SDR color status', () => {
  it('parses the exact supported wire descriptor', () => {
    expect(parseColorManagementStatusV1(FIXED_VIDEO_OUTPUT_REC709_LIMITED)).toEqual(FIXED_VIDEO_OUTPUT_REC709_LIMITED)
  })
  it('parses unsupported and not_applicable variants', () => {
    expect(parseColorManagementStatusV1({ status: 'unsupported', reason: 'hdr_unsupported' })).toEqual({ status: 'unsupported', reason: 'hdr_unsupported' })
    expect(parseColorManagementStatusV1({ status: 'not_applicable' })).toEqual({ status: 'not_applicable' })
  })
  it.each([
    null,
    { status: 'future' },
    { status: 'unsupported', reason: 'future_reason' },
    { status: 'not_applicable', extra: true },
    { status: 'supported', descriptor: { primaries: 'bt709', transfer: 'bt709', matrix: 'bt709', range: 'limited', pixelModel: 'yuv' } },
    { status: 'supported', provenance: 'probe', descriptor: { primaries: 'bt709', transfer: 'bt709', matrix: 'bt709', range: 'limited', pixelModel: 'yuv' } },
    { status: 'supported', descriptor: { primaries: 'bt709', transfer: 'srgb', matrix: 'rgb', range: 'limited', pixelModel: 'rgb' } },
    { status: 'supported', descriptor: { primaries: 'bt2020', transfer: 'bt709', matrix: 'bt709', range: 'limited', pixelModel: 'yuv' } },
  ])('strictly rejects malformed status %#', value => expect(() => parseColorManagementStatusV1(value)).toThrow())
  it('presents malformed API data without throwing', () => {
    expect(() => sourceColorStatus({ status: 'future' })).not.toThrow()
    expect(sourceColorStatus(undefined).verified).toBe(false)
  })
  it('marks browser-decoded sRGB as explicit but unverified source metadata', () => {
    expect(sourceColorStatus(BROWSER_DECODED_SRGB_STATUS)).toMatchObject({ label: 'Источник: browser sRGB', verified: false })
  })
  it('parses exact Rust JSON provenance and warns for legacy assumptions', () => {
    const rust = { status: 'supported', descriptor: { primaries: 'bt709', transfer: 'bt709', matrix: 'bt709', range: 'limited', pixelModel: 'yuv', chromaLocation: 'left' }, provenance: 'legacy_assumed_bt709' } as const
    expect(parseColorManagementStatusV1(rust)).toEqual(rust)
    expect(sourceColorStatus(rust)).toMatchObject({ verified: false })
    expect(sourceColorStatus(rust).warning).toContain('legacy')
  })
  it('uses a format-aware output presentation', () => {
    expect(outputColorStatus('mp4').label).toContain('Rec.709 limited')
    expect(outputColorStatus('png').label).toContain('sRGB full')
    expect(outputColorStatus('jpg').label).toContain('sRGB full')
    expect(outputColorStatus('gif').warning).toBeTruthy()
    expect(outputColorStatus('mp3').label).toContain('N/A')
  })
})
