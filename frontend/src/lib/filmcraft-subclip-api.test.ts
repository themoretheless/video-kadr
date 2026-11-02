import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftSubclip, getFilmcraftSubclipDefaults } from './api'
afterEach(() => vi.unstubAllGlobals())
const parent = '9007199254740993'; const item = '18446744073709551614'
const document = '{"id":18446744073709551615}'
const input = { document, context: { activeSequence: null, sourceItem: parent, selection: [], projectSelection: [parent] } }
const subclip = { parentId: parent, startTicks: '9007199254740997', endTicks: '9007199254740999', restrictTrims: true }
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [], projectItems: [{ id: parent, name: 'Parent', kind: 'Media' }, { id: item, name: 'Subclip', kind: 'Subclip', subclip }] }
const info = { itemId: item, parentId: parent, created: true, converted: false, bindingSourceId: null }
const reply = { result: { item }, subclipEdit: info, context: { projectSelection: [item] }, documentText: document }
it('creates a subclip with exact text, authoritative project selection and one import', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftSubclip('owner', 'clip.makeSubclip', input)).toEqual({ ...manifest, subclipEdit: info, subclipProjectSelection: [item] })
  expect(fetcher).toHaveBeenCalledTimes(2); expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it('retains direct-parent binding provenance for master conversion without using file paths as authority', async () => {
  const converted = { ...info, created: false, converted: true, bindingSourceId: parent }
  const project = { ...manifest, projectItems: [{ id: parent, name: 'Parent', kind: 'Media' }, { id: item, name: 'Converted', kind: 'Media' }] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ ...reply, result: { item, converted: true }, subclipEdit: converted, context: { projectSelection: [parent] } }))).mockResolvedValueOnce(new Response(JSON.stringify(project))); vi.stubGlobal('fetch', fetcher)
  expect((await editFilmcraftSubclip('owner', 'clip.editSubclip', { ...input, item, convertToMaster: true })).subclipEdit).toEqual(converted)
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it('allows editing broken or cyclic subclips when no conversion is requested', async () => {
  const cyclic = { ...info, parentId: item, created: false }; const project = { ...manifest, projectItems: [{ ...manifest.projectItems[1], subclip: { ...subclip, parentId: item } }] }
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ ...reply, result: { item, converted: false }, subclipEdit: cyclic, context: { projectSelection: [parent] } }))).mockResolvedValueOnce(new Response(JSON.stringify(project))))
  expect((await editFilmcraftSubclip('owner', 'clip.editSubclip', { ...input, item })).subclipEdit.parentId).toBe(item)
})
it.each([{ subclipEdit: undefined }, { subclipEdit: { ...info, itemId: Number(item) } }, { subclipEdit: { ...info, created: false } }, { subclipEdit: { ...info, bindingSourceId: parent } }, { context: { projectSelection: [parent] } }, { result: { item: parent } }, { result: { item, converted: false } }])('refuses inconsistent subclip metadata before import (%j)', async patch => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...reply, ...patch }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftSubclip('owner', 'clip.makeSubclip', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('rejects an imported snapshot that contradicts subclip ancestry', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify({ ...manifest, projectItems: [{ ...manifest.projectItems[1], subclip: { ...subclip, parentId: item } }] }))))
  await expect(editFilmcraftSubclip('owner', 'clip.makeSubclip', input)).rejects.toThrow('identities')
})
const defaults = { name: 'Source.Subclip', ...subclip, startFrame: '17', endFrame: '21', fps: 25, frameRate: { num: 25, den: 1 } }
it('reads original dialog defaults without importing or editing the document', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(defaults))); vi.stubGlobal('fetch', fetcher)
  expect(await getFilmcraftSubclipDefaults('owner', input)).toEqual(defaults); expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([{ startTicks: Number(defaults.startTicks) }, { parentId: '18446744073709551616' }, { endFrame: '-0' }, { frameRate: { num: 0, den: 1 } }, { restrictTrims: null }])('rejects malformed defaults (%j)', async patch => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...defaults, ...patch })))); await expect(getFilmcraftSubclipDefaults('owner', input)).rejects.toThrow()
})
it('does not import a late edit reply after observer abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify(reply)) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftSubclip('owner', 'clip.makeSubclip', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
