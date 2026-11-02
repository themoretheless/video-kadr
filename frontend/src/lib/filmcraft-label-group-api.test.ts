import { afterEach, expect, it, vi } from 'vitest'
import { selectFilmcraftLabelGroup } from './api'
afterEach(() => vi.unstubAllGlobals())
const input = { document: '{"id":18446744073709551615}', sequence: '9007199254740993', context: { selection: [], projectSelection: ['18446744073709551615'] } }
it('retains exact IDs, tree order and duplicate references without importing a document', async () => {
  const result = { items: ['18446744073709551615', '9007199254740993', '18446744073709551615'] }
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result }))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await selectFilmcraftLabelGroup('owner', input, controller.signal)).toEqual(result)
  expect(fetcher).toHaveBeenCalledTimes(1)
  const init = fetcher.mock.calls[0][1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(init.signal).toBe(controller.signal)
  expect(JSON.parse(init.body as string)).toEqual({ command: 'edit.selectLabelGroup', params: input })
})
it.each([{ clips: [] }, { clips: ['0', '18446744073709551615'] }, { items: [] }])('accepts source selection %j', async result => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ result }))))
  expect(await selectFilmcraftLabelGroup('owner', { ...input, context: { ...input.context, selection: 'clips' in result ? ['9007199254740993'] : [] } })).toEqual(result)
})
it.each([null, {}, { clips: [], items: [] }, { clips: [9007199254740992] }, { clips: ['18446744073709551616'] }, { items: ['01'] }, { items: ['-1'] }, { items: null }, { clips: [], extra: true }])('refuses malformed result %j without replay', async result => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result }))); vi.stubGlobal('fetch', fetcher)
  await expect(selectFilmcraftLabelGroup('owner', { ...input, context: { ...input.context, selection: result && 'clips' in result ? ['9007199254740993'] : [] } })).rejects.toThrow('Label group')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('refuses completion after observation abort without importing or replaying', async () => {
  const controller = new AbortController()
  const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ result: { items: [] } })) }); vi.stubGlobal('fetch', fetcher)
  await expect(selectFilmcraftLabelGroup('owner', input, controller.signal)).rejects.toThrow('Aborted')
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it.each([true, false])('rejects the wrong source selection scope (timeline=%s)', async timeline => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result: timeline ? { items: [] } : { clips: [] } }))); vi.stubGlobal('fetch', fetcher)
  await expect(selectFilmcraftLabelGroup('owner', { ...input, context: { ...input.context, selection: timeline ? ['9007199254740993'] : [] } })).rejects.toThrow('selection scope')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
