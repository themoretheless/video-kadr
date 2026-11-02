import { afterEach, expect, it, vi } from 'vitest'
import { setNativeProjectMediaPlaceholder, removeNativeProjectMedia, importConcatProject, type NativeNestedProject } from './api'
import { createComposition } from './composition/commands'
import { buildCompositionWireDocument } from './composition/payload'
afterEach(() => vi.unstubAllGlobals())
const project: NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content: buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 24, backgroundColor: '#000000' })), instances: [] } } }
it('sends one immutable native request and preserves reference identity for no-op replies', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ project: { compositions: project.compositions, schemaVersion: 1, root: 'root' }, applied: false }))); vi.stubGlobal('fetch', fetcher)
  expect(await setNativeProjectMediaPlaceholder(project, 'missing', true)).toBe(project)
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ project, sourceId: 'missing', placeholder: true }); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('rejects unrelated project identity and late aborted observations', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ project: { ...project, root: 'other' }, applied: true }))))
  await expect(setNativeProjectMediaPlaceholder(project, 'source', true)).rejects.toThrow('invalid project')
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ project, applied: true })) }); vi.stubGlobal('fetch', fetcher)
  await expect(setNativeProjectMediaPlaceholder(project, 'source', true, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})

it('removes registered media with an immutable bounded native command and preserves no-op identity', async () => {
  const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify({project,applied:false})));vi.stubGlobal('fetch',fetcher)
  expect(await removeNativeProjectMedia(project,'missing')).toBe(project)
  expect(fetcher.mock.calls[0][0]).toContain('/api/compositions/nested-media-remove')
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({project,sourceId:'missing'})
})

it('imports Concat with stable logical media IDs while sending physical library descriptors', async () => {
  const document={version:1,media:[{id:'slot-a',kind:'image'},{id:'slot-b',kind:'image'}]}
  const physical={id:'library-image',kind:'image' as const,durationTicks:0,width:16,height:16,hasAudio:false}
  const mediaBindings={'slot-a':physical,'slot-b':physical}
  const fetcher=vi.fn().mockImplementation(()=>Promise.resolve(new Response(JSON.stringify({schemaVersion:1,status:'converted',project,diagnostics:[],sourceDocument:document,timelineIds:{}}))))
  vi.stubGlobal('fetch',fetcher)
  await importConcatProject(document,mediaBindings,undefined,'session',{},true)
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({schemaVersion:1,document,mediaBindings,preserveMediaIds:true})
  await importConcatProject(document,mediaBindings,undefined,'session',{},false)
  expect(JSON.parse(fetcher.mock.calls[1][1].body).preserveMediaIds).toBe(false)
  expect(mediaBindings['slot-a'].id).toBe('library-image')
})
