import 'fake-indexeddb/auto'
import { describe, expect, it } from 'vitest'
import { BrowserExportQueue } from './browser-export-queue'
import { expandExportBatch } from './domain/export-variants'

const definitions = () => expandExportBatch({ id: 'batch', source: { assetRef: 'asset', fingerprint: 'a'.repeat(64) }, basePayload: { format: 'mp4' }, variants: [
  { id: 'one', label: 'One', overrides: { quality: 20 } }, { id: 'two', label: 'Two', overrides: { quality: 30 } },
] })

describe('persistent browser export queue', () => {
  it('atomically persists definitions and claims FIFO globally sequentially', async () => {
    const name = `exports-${crypto.randomUUID()}`, first = new BrowserExportQueue(name), second = new BrowserExportQueue(name)
    await first.enqueue(definitions())
    await expect(first.enqueue([definitions()[0]!])).rejects.toThrow('duplicate')
    expect((await second.list()).map(job => job.definition.variantId)).toEqual(['one', 'two'])
    const claimed = await first.claim(1000); expect(claimed?.definition.variantId).toBe('one')
    expect(await second.claim(1000)).toBeNull()
    await first.close(); await second.close()
  })

  it('fences stale generations and restarts interrupted work from zero', async () => {
    let now = 100; const name = `exports-${crypto.randomUUID()}`, queue = new BrowserExportQueue(name, () => now)
    await queue.enqueue(definitions())
    const old = (await queue.claim(10))!
    now = 111
    expect(await queue.claim(10)).toBeNull()
    expect(await queue.recoverRunning()).toBe(1)
    const interrupted = (await queue.list())[0]!
    expect(interrupted).toMatchObject({ state: 'interrupted', attempt: 1 })
    expect('progress' in interrupted).toBe(false)
    expect(await queue.complete(old.definition.id, old.generation, { url: 'stale' })).toBe(false)
    expect(await queue.retry(old.definition.id)).toBe(true)
    now++
    const restarted = (await queue.claim(10))!
    expect(restarted).toMatchObject({ state: 'running', attempt: 2 })
    expect(restarted.generation).toBeGreaterThan(old.generation)
    await queue.close()
  })

  it('supports permission, retry, cancellation and lease fencing', async () => {
    let now = 1; const queue = new BrowserExportQueue(`exports-${crypto.randomUUID()}`, () => now)
    await queue.enqueue(definitions())
    const first = (await queue.claim(5))!
    expect(await queue.requirePermission(first.definition.id, first.generation, 'relink', 'source')).toBe(true)
    expect((await queue.list())[0]).toMatchObject({ state: 'permission_required', recoveryKind: 'source' })
    expect(await queue.retry(first.definition.id)).toBe(true)
    expect((await queue.list())[0]).not.toHaveProperty('recoveryKind')
    const rerun = (await queue.claim(5))!; now = 99
    expect(await queue.heartbeat(rerun.definition.id, rerun.generation)).toBe(false)
    expect(await queue.progress(rerun.definition.id, rerun.generation, 50, 'encoding')).toBe(false)
    expect(await queue.cancel(rerun.definition.id)).toBe(true)
    expect(await queue.complete(rerun.definition.id, rerun.generation, {})).toBe(false)
    expect(await queue.retry(rerun.definition.id)).toBe(true)
    expect((await queue.list())[0]).toMatchObject({ state: 'queued', generation: rerun.generation + 1 })
    await queue.close()
  })

  it('does not recover a live lease owned by another tab', async () => {
    let now = 10
    const name = `exports-${crypto.randomUUID()}`
    const owner = new BrowserExportQueue(name, () => now)
    const startup = new BrowserExportQueue(name, () => now)
    await owner.enqueue(definitions())
    const live = (await owner.claim(100))!
    expect(await startup.recoverRunning()).toBe(0)
    expect((await startup.list())[0]).toMatchObject({ state: 'running', generation: live.generation, leaseUntil: 110 })
    now = 111
    expect(await startup.recoverRunning()).toBe(1)
    expect((await owner.list())[0]).toMatchObject({ state: 'interrupted', generation: live.generation + 1 })
    await owner.close(); await startup.close()
  })

  it('invalidates completed session blob results after reload', async () => {
    const now = 1
    const name = `exports-${crypto.randomUUID()}`
    const owner = new BrowserExportQueue(name, () => now)
    await owner.enqueue(definitions())
    const claimed = (await owner.claim(10))!
    expect(await owner.complete(claimed.definition.id, claimed.generation, { url: 'blob:dead-after-reload', filename: 'x.mp4' })).toBe(true)
    const reopened = new BrowserExportQueue(name, () => now)
    expect(await reopened.recoverRunning()).toBe(1)
    const recovered = (await reopened.list())[0]!
    expect(recovered).toMatchObject({ state: 'interrupted' })
    expect('result' in recovered).toBe(false)
    expect(await reopened.retry(recovered.definition.id)).toBe(true)
    await owner.close(); await reopened.close()
  })
})
