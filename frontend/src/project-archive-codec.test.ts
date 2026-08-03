import { describe, expect, it } from 'vitest'
import { sha256 } from '@noble/hashes/sha2.js'
import golden from '../../fixtures/portable-archive/v1-golden.json'
import { createProjectDocumentFromLegacy } from './project-schema'
import {
  buildPortableArchive,
  PortableArchiveError,
  readPortableArchive,
  VKADR_MAGIC,
} from './project-archive-codec'

const mediaSha = '22'.repeat(32)

function project() {
  return createProjectDocumentFromLegacy(
    'media-1',
    'Portable',
    { id: 'media-1', filename: 'clip.mp4', duration: 1, width: 16, height: 16, fingerprint: mediaSha },
    {},
  )
}

describe('VKADR v1 codec', () => {
  it('reads the complete byte-for-byte fixture emitted and checked by Rust', async () => {
    const bytes = new Uint8Array(golden.vkadrHex.match(/../g)!.map(value => Number.parseInt(value, 16)))
    const decoded = await readPortableArchive(new Blob([bytes]))
    expect(decoded.manifest.rootHash).toBe(golden.rootHash)
    expect(decoded.manifest.project).toEqual(golden.project)
  })

  it('round-trips ordered payloads with the backend-compatible header', async () => {
    const source = new Blob(['source bytes'], { type: 'video/mp4' })
    const sourceSha = Array.from(sha256(new TextEncoder().encode('source bytes')), byte => byte.toString(16).padStart(2, '0')).join('')
    const built = await buildPortableArchive(project(), [{
      path: `media/${sourceSha}.mp4`,
      kind: 'media',
      blob: source,
    }])
    const header = new Uint8Array(await built.blob.slice(0, 12).arrayBuffer())
    expect([...header.slice(0, 8)]).toEqual([...VKADR_MAGIC])
    expect(new DataView(header.buffer).getUint32(8, false)).toBeGreaterThan(0)

    const decoded = await readPortableArchive(built.blob)
    expect(decoded.project.name).toBe('Portable')
    expect(decoded.manifest.entries.map(entry => entry.path)).toEqual([
      `media/${sourceSha}.mp4`,
      'project.json',
    ])
    expect(await decoded.payloads.get(`media/${sourceSha}.mp4`)!.text()).toBe('source bytes')
  })

  it('rejects wrong magic, truncation, trailing bytes and payload corruption', async () => {
    const built = await buildPortableArchive(project(), [])
    const bytes = new Uint8Array(await built.blob.arrayBuffer())

    const wrongMagic = bytes.slice(); wrongMagic[0] ^= 1
    await expect(readPortableArchive(new Blob([wrongMagic]))).rejects.toMatchObject({ code: 'wrong_format' })
    await expect(readPortableArchive(new Blob([bytes.slice(0, -1)]))).rejects.toMatchObject({ code: 'truncated' })
    await expect(readPortableArchive(new Blob([bytes, new Uint8Array([0])]))).rejects.toMatchObject({ code: 'trailing_bytes' })
    const corrupt = bytes.slice(); corrupt[corrupt.length - 1] ^= 1
    await expect(readPortableArchive(new Blob([corrupt]))).rejects.toMatchObject({ code: 'checksum_mismatch' })
  })

  it('honours cancellation before publishing or installing bytes', async () => {
    const controller = new AbortController(); controller.abort()
    await expect(buildPortableArchive(project(), [], { signal: controller.signal }))
      .rejects.toBeInstanceOf(PortableArchiveError)
  })
})
