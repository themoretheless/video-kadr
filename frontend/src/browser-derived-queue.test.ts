import 'fake-indexeddb/auto'
import { describe, expect, it } from 'vitest'
import { BrowserDerivedQueue, type DerivedTaskInput } from './browser-derived-queue'

const task = (id: string, projectId = 'p', dependencies: string[] = [], priority = 0): DerivedTaskInput => ({ id, projectId, dependencies, priority, kind: id.includes('proxy') ? 'proxy' : 'probe', idempotencyKey: `key-${id}`, payloadVersion: 1, payload: { id } })
const queue = (now = 1_000) => new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => now)

describe('BrowserDerivedQueue', () => {
  it('validates DAG atomically and claims dependencies in order', async () => {
    const q = queue(); await q.enqueueGraph([task('source'), task('proxy', 'p', ['source'])])
    expect((await q.claim())?.id).toBe('source')
    await expect(q.enqueueGraph([task('bad', 'p', ['missing'])])).rejects.toThrow('missing dependency')
    expect((await q.list()).some(t => t.id === 'bad')).toBe(false)
    const source = (await q.list()).find(t => t.id === 'source')!; await q.complete(source.id, source.generation, {})
    expect((await q.claim())?.id).toBe('proxy')
  })

  it('rejects a dependency cycle without publishing either node', async () => {
    const q = queue()
    await expect(q.enqueueGraph([task('a', 'p', ['b']), task('b', 'p', ['a'])])).rejects.toThrow('dependency cycle')
    expect(await q.list()).toEqual([])
  })

  it('recovers expired running work and fences stale generations', async () => {
    let now = 1_000; const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => now)
    await q.enqueueGraph([task('a')]); const first = (await q.claim(10))!; now = 1_011
    expect(await q.recoverExpired()).toBe(1); const second = (await q.claim(10))!
    expect(second.generation).toBe(first.generation + 1)
    expect(await q.complete(first.id, first.generation, 'stale')).toBe(false)
    expect(await q.complete(second.id, second.generation, 'ok')).toBe(true)
  })

  it('deduplicates concurrent enqueue by persistent idempotency key', async () => {
    const q = queue(); const input = task('one')
    const results = await Promise.allSettled(Array.from({ length: 20 }, () => q.enqueueGraph([input])))
    expect(results.some(r => r.status === 'fulfilled')).toBe(true)
    expect((await q.list()).filter(t => t.idempotencyKey === input.idempotencyKey)).toHaveLength(1)
  })

  it('persists priority with aging and rotates projects fairly', async () => {
    let now = 1_000; const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => now)
    await q.enqueueGraph([task('low', 'a', [], 0), task('high', 'a', [], 5), task('other', 'b', [], 0)])
    expect((await q.claim())?.id).toBe('high')
    expect((await q.claim())?.projectId).toBe('b')
    now += 6 * 60_000
    expect((await q.claim())?.id).toBe('low')
  })

  it('propagates queued cancellation but fences a running child', async () => {
    const q = queue(); await q.enqueueGraph([task('root'), task('child', 'p', ['root']), task('grand', 'p', ['child'])])
    expect(await q.cancel('root')).toBe(true)
    expect((await q.list()).map(t => t.state)).toEqual(['cancelled', 'cancelled', 'cancelled'])
  })

  it('does not let completion win after durable running cancellation', async () => {
    const q = queue(); await q.enqueueGraph([task('root')]); const running = (await q.claim())!
    await q.cancel(running.id)
    expect(await q.complete(running.id, running.generation, {})).toBe(false)
    expect((await q.list())[0]?.state).toBe('cancelled')
  })

  it('atomically cancels a running executor without waiting for lease recovery', async () => {
    const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => Date.now())
    await q.enqueueGraph([task('running')])
    let started!: () => void
    const didStart = new Promise<void>(resolve => { started = resolve })
    const run = q.runNext(async (_task, signal) => {
      started()
      await new Promise<void>((resolve, reject) => {
        signal.addEventListener('abort', () => reject(new DOMException('cancelled', 'AbortError')), { once: true })
      })
      return {}
    })
    await didStart
    await q.cancel('running', 'p')
    expect((await q.list())[0]).toMatchObject({ state: 'cancelled' })
    expect((await q.list())[0]?.leaseUntil).toBeUndefined()
    await run
    expect((await q.list())[0]?.state).toBe('cancelled')
  })

  it('detaches one project from a shared DAG without cancelling the other consumer', async () => {
    const q = queue()
    await q.enqueueGraph([task('root-a', 'a'), task('child-a', 'a', ['root-a'])])
    await q.enqueueGraph([
      { ...task('root-b', 'b'), idempotencyKey: 'key-root-a', payload: { id: 'root-a' } },
      { ...task('child-b', 'b', ['root-b']), idempotencyKey: 'key-child-a', payload: { id: 'child-a' } },
    ])
    await q.cancel('root-a', 'a')
    const remaining = await q.list()
    expect(remaining).toHaveLength(2)
    expect(remaining.every(item => item.consumerProjectIds.join() === 'b')).toBe(true)
    expect(remaining.every(item => item.state !== 'cancelled')).toBe(true)
    await q.cancel('root-a', 'b')
    expect((await q.list()).every(item => item.state === 'cancelled')).toBe(true)
  })

  it('does not resurrect a cancelled project during startup reconciliation', async () => {
    const name = `test-${crypto.randomUUID()}`
    const q = new BrowserDerivedQueue(name, () => 1_000)
    const input = task('source', 'media:source')
    await q.enqueueGraph([input])
    await q.cancel('source', 'media:source')
    await q.close()
    const reopened = new BrowserDerivedQueue(name, () => 2_000)
    expect(await reopened.enqueueGraph([{ ...input, id: 'startup-alias' }])).toEqual([])
    expect((await reopened.list())[0]).toMatchObject({ state: 'cancelled', consumerProjectIds: [] })
  })

  it('persists priority with optimistic revision and rejects stale updates', async () => {
    const q = queue(); await q.enqueueGraph([task('a')])
    const updated = await q.reprioritize('a', 20, 0)
    expect(updated).toMatchObject({ priority: 20, priorityRevision: 1 })
    await expect(q.reprioritize('a', 30, 0)).rejects.toThrow('priority revision conflict')
    await q.close()
    const reopened = new BrowserDerivedQueue((q as unknown as { name: string }).name, () => 1_000)
    expect((await reopened.list())[0]).toMatchObject({ priority: 20, priorityRevision: 1 })
  })

  it('renews only the current lease generation and supports permission resume', async () => {
    let now = 1_000; const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => now)
    await q.enqueueGraph([task('a')]); const running = (await q.claim(10))!
    now = 1_005
    expect(await q.heartbeat('a', running.generation, 20)).toBe(true)
    expect(await q.heartbeat('a', running.generation - 1, 20)).toBe(false)
    expect(await q.requirePermission('a', running.generation, 'grant access')).toBe(true)
    expect(await q.resumePermission('a')).toBe(true)
    expect((await q.claim())?.id).toBe('a')
  })

  it('donates downstream priority to a required dependency', async () => {
    const q = queue()
    await q.enqueueGraph([task('unrelated', 'p', [], 5), task('root', 'p', [], -10), task('interactive', 'p', ['root'], 10)])
    expect((await q.claim())?.id).toBe('root')
  })

  it('gives an old background task a finite starvation bound', async () => {
    let now = 1_000; const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => now)
    await q.enqueueGraph([task('old', 'p', [], -100)])
    now += 201 * 60_000
    await q.enqueueGraph([task('new', 'p', [], 100)])
    expect((await q.claim())?.id).toBe('old')
  })

  it('rejects semantic idempotency collisions and oversized graphs atomically', async () => {
    const q = queue(); await q.enqueueGraph([task('a')])
    await expect(q.enqueueGraph([{ ...task('alias'), idempotencyKey: 'key-a', kind: 'waveform' }])).rejects.toThrow('different task semantics')
    await expect(q.enqueueGraph(Array.from({ length: 257 }, (_, index) => task(`node-${index}`)))).rejects.toThrow('node limit')
    expect(await q.list()).toHaveLength(1)
  })

  it('allows only one of two tab schedulers to claim the same artifact', async () => {
    const name = `test-${crypto.randomUUID()}`
    const first = new BrowserDerivedQueue(name, () => 1_000)
    const second = new BrowserDerivedQueue(name, () => 1_000)
    await first.enqueueGraph([task('shared')])
    const claims = await Promise.all([first.claim(), second.claim()])
    expect(claims.filter(Boolean)).toHaveLength(1)
  })

  it('keeps a long-running executor lease alive until fenced completion', async () => {
    const q = new BrowserDerivedQueue(`test-${crypto.randomUUID()}`, () => Date.now())
    await q.enqueueGraph([task('long')])
    const running = q.runNext(async () => {
      await new Promise(resolve => setTimeout(resolve, 35))
      return { ok: true }
    }, 30)
    await new Promise(resolve => setTimeout(resolve, 5))
    const firstLease = (await q.list())[0]!.leaseUntil!
    await new Promise(resolve => setTimeout(resolve, 15))
    expect((await q.list())[0]!.leaseUntil).toBeGreaterThan(firstLease)
    await running
    expect((await q.list())[0]!.state).toBe('succeeded')
  })
})
