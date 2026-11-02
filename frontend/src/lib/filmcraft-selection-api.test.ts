import { afterEach, expect, it, vi } from 'vitest'
import { selectFilmcraftSelection, type FilmcraftSelectionCommand } from './api'
afterEach(() => vi.unstubAllGlobals())
const a = '9007199254740993', b = '18446744073709551615'
const input = { document: '{"id":18446744073709551615}', sequence: a, context: { selection: [a], projectSelection: [b] } }
it.each(['edit.selectAllMatching', 'project.selectAll', 'project.deselectAll', 'project.select'] as const)('adopts exact %s context without importing a project', async command => {
  const project = command === 'project.deselectAll' ? [] : command === 'project.select' ? [b, a, b] : [a, b]
  const clips = command === 'edit.selectAllMatching' ? [b, a] : [a]
  const result = command === 'edit.selectAllMatching' ? { clips } : command === 'project.selectAll' ? { selected: project.length } : null
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result, context: { selection: clips, projectSelection: command === 'edit.selectAllMatching' ? [b] : project } }))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController(); const params = { ...input, ...(command === 'project.select' ? { items: project } : {}) }
  expect(await selectFilmcraftSelection('owner', command, params, controller.signal)).toEqual(command === 'edit.selectAllMatching' ? { clips } : { items: project })
  expect(fetcher).toHaveBeenCalledTimes(1)
  const init = fetcher.mock.calls[0][1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner'); expect(init.signal).toBe(controller.signal)
  expect(JSON.parse(init.body as string)).toEqual({ command, params })
})
it.each([
  ['edit.selectAllMatching', { result: { clips: [a] }, context: { selection: [a], projectSelection: [] } }],
  ['edit.selectAllMatching', { result: { clips: [b] }, context: { selection: [a], projectSelection: [b] } }],
  ['project.selectAll', { result: { selected: 1 }, context: { selection: [], projectSelection: [b] } }],
  ['project.selectAll', { result: { selected: 2 }, context: { selection: [a], projectSelection: [b] } }],
  ['project.selectAll', { result: { selected: 2 }, context: { selection: [a], projectSelection: [b, b] } }],
  ['project.deselectAll', { result: null, context: { selection: [a], projectSelection: [b] } }],
  ['project.select', { result: null, context: { selection: [a], projectSelection: [b] } }],
  ['project.select', { result: { selected: 0 }, context: { selection: [a], projectSelection: [] } }],
  ['project.selectAll', { result: { selected: 1 }, context: { selection: [a], projectSelection: [Number(b)] } }],
  ['project.selectAll', { result: { selected: 1 }, context: { selection: [a], projectSelection: ['18446744073709551616'] } }],
  ['project.selectAll', { result: null }]
] as const)('refuses malformed or mismatched %s replies without replay', async (command, raw) => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(raw))); vi.stubGlobal('fetch', fetcher)
  await expect(selectFilmcraftSelection('owner', command as FilmcraftSelectionCommand, input)).rejects.toThrow('Selection command')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('refuses completion after observation abort without importing', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ result: null, context: { selection: [a], projectSelection: [] } })) }); vi.stubGlobal('fetch', fetcher)
  await expect(selectFilmcraftSelection('owner', 'project.deselectAll', input, controller.signal)).rejects.toThrow('Aborted')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
