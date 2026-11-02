import { afterEach, expect, it, vi } from 'vitest'
import { fillConcatMediaSlot } from './api'
afterEach(() => vi.unstubAllGlobals())
const entry = { label: 'New image', source: { id: 'registered', kind: 'image' as const, durationTicks: 0, width: 16, height: 8, hasAudio: false } }
it('sends only the registered source ID and uses fresh server metadata', async () => {
  const original = { version: 1, media: [{ id: 'slot', placeholder: true }], timelines: [] }, filled = { ...original, media: [{ id: 'slot', placeholder: false }] }
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ schemaVersion: 1, document: filled, applied: true,source:entry.source }))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await fillConcatMediaSlot(original, 'slot', entry, controller.signal, 'session')).toEqual({document:filled,source:entry.source})
  expect(fetcher.mock.calls[0][0]).toContain('/api/compositions/concat-project/fill-registered-slot')
  const request = fetcher.mock.calls[0][1]
  expect(request.signal).toBe(controller.signal); expect(request.headers.Authorization).toBe('Bearer session')
  expect(JSON.parse(request.body)).toEqual({document:original,mediaId:'slot',sourceId:'registered'})
  expect(original.media[0].placeholder).toBe(true)
})
it('rejects a no-op response or a result observed after abort', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ schemaVersion: 1, document: {}, applied: false }))))
  await expect(fillConcatMediaSlot({}, 'slot', entry)).rejects.toThrow('Некорректный ответ')
  const controller = new AbortController()
  vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ schemaVersion: 1, document: {}, applied: true })) }))
  await expect(fillConcatMediaSlot({}, 'slot', entry, controller.signal)).rejects.toThrow('Aborted')
})

it('rejects a descriptor for another registered source',async()=>{
 vi.stubGlobal('fetch',vi.fn().mockResolvedValue(new Response(JSON.stringify({schemaVersion:1,document:{},applied:true,source:{...entry.source,id:'other'}}))))
 await expect(fillConcatMediaSlot({},'slot',entry)).rejects.toThrow('Некорректный ответ')
})
