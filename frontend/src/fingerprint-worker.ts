/// <reference lib="webworker" />

import { sha256 } from '@noble/hashes/sha2.js'

const HASH_CHUNK_BYTES = 4 * 1024 * 1024

self.postMessage({ ready: true })

self.onmessage = async (event: MessageEvent<Blob>) => {
  try {
    const blob = event.data
    const hasher = sha256.create()
    for (let offset = 0; offset < blob.size; offset += HASH_CHUNK_BYTES) {
      const chunk = blob.slice(offset, Math.min(blob.size, offset + HASH_CHUNK_BYTES))
      hasher.update(new Uint8Array(await chunk.arrayBuffer()))
      self.postMessage({ progress: Math.min(blob.size, offset + HASH_CHUNK_BYTES) })
    }
    const digest = [...hasher.digest()].map((byte) => byte.toString(16).padStart(2, '0')).join('')
    self.postMessage({ digest })
  } catch (error) {
    self.postMessage({ error: error instanceof Error ? error.message : String(error) })
  }
}
