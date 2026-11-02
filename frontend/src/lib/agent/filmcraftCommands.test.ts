import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as api from '../api'
import { executeAgentRequest } from './runtime.svelte'
import { agentActivity } from './activity.svelte'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests } from '../state/composition/core.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory } from '../state/nestedComposition.svelte'

beforeEach(() => { closeNestedComposition(); setCompositionStorageForTests(null); resetCompositionForTests() })
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); closeNestedComposition() })
const call = (tool: string, args: Record<string, unknown>, access: Parameters<typeof executeAgentRequest>[3] = { token: 'owner' }) => executeAgentRequest('source-agent', tool, args, access)

it('routes source discovery and run to Rust even with a nested project open, without composition Undo', async () => {
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: {} }
  const ordinary = JSON.stringify(compositionState.document)
  const nested = JSON.stringify(nestedComposition.document)
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValueOnce([{ id: 'export.queue.list' }]).mockResolvedValueOnce({ running: false, items: [] })
  expect(await call('command_list', { filter: 'queue', enabled_only: true })).toEqual([{ id: 'export.queue.list' }])
  expect(request).toHaveBeenNthCalledWith(1, 'owner', 'command.list', { filter: 'queue', enabled_only: true }, expect.any(AbortSignal))
  await call('command_run', { id: 'export.queue.list', params: null })
  expect(request).toHaveBeenNthCalledWith(2, 'owner', 'export.queue.list', {}, expect.any(AbortSignal))
  expect(JSON.stringify(compositionState.document)).toBe(ordinary)
  expect(JSON.stringify(nestedComposition.document)).toBe(nested)
  expect(nestedHistory.past).toHaveLength(0)
  expect(agentActivity.activities.find(item => item.tool === 'command_list')?.kind).toBe('read')
})
it('refuses missing authentication, expired contexts and malformed tool envelopes before dispatch', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand')
  await expect(call('command_list', {}, {})).rejects.toThrow('authenticated owner')
  await expect(call('command_list', {}, { token: 'owner', expiresAt: Date.now() - 1 })).rejects.toThrow('expired')
  await expect(call('command_run', { id: 'export.queue.list', params: [] })).rejects.toThrow('object')
  await expect(call('command_run', { params: {} })).rejects.toThrow('id is required')
  await expect(call('command_list', { filtr: 'queue' })).rejects.toThrow('Unknown')
  expect(request).not.toHaveBeenCalled()
})
it('treats late completion after an owner/context change as unconfirmed and never replays', async () => {
  let live = true
  let resolve!: (value: unknown) => void
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockImplementation(() => new Promise(accept => { resolve = accept }))
  const pending = call('command_run', { id: 'export.queue.start' }, { token: 'owner', isActive: () => live })
  await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(1))
  live = false; resolve({ running: true })
  await expect(pending).rejects.toThrow('completion unconfirmed')
  expect(request).toHaveBeenCalledTimes(1)
})
it('aborts the pending transport on disconnect and releases its observation timer', async () => {
  vi.useFakeTimers()
  let live = true
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockImplementation((_token, _command, _params, signal) => new Promise((_resolve, reject) => {
    signal!.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true })
  }))
  const pending = call('command_run', { id: 'export.queue.start', params: { wait: true } }, { token: 'owner', isActive: () => live })
  await vi.advanceTimersByTimeAsync(0)
  const rejected = expect(pending).rejects.toThrow('completion unconfirmed')
  live = false
  await vi.advanceTimersByTimeAsync(101)
  await rejected
  expect(request.mock.calls[0]![3]!.aborted).toBe(true)
  expect(request).toHaveBeenCalledTimes(1)
  expect(vi.getTimerCount()).toBe(0)
})

it('refuses rounded source integers and transports serialized snapshots without parsing them', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ items: [] })
  await expect(call('command_run', { id: 'export.queue.add', params: { sequence: Number.MAX_SAFE_INTEGER + 1 } })).rejects.toThrow('decimal strings')
  await expect(call('command_run', { id: 'export.queue.add', params: { document: { originalId: Number.MAX_SAFE_INTEGER + 1 } } })).rejects.toThrow('serialized document')
  expect(request).not.toHaveBeenCalled()
  const document = '{"originalId":18446744073709551615}'
  const params = { document, sequence: '18446744073709551615' }
  await call('command_run', { id: 'export.queue.add', params })
  expect(request).toHaveBeenCalledWith('owner', 'export.queue.add', params, expect.any(AbortSignal))
})

it('sends the whole batch and one deadline to Rust, retaining structured partial failures', async () => {
  const result = { completed: 1, failed: 1, results: [{ ok: true, result: { name: 'Saved' } }, { ok: false, error: 'unknown command' }] }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  const expiresAt = Date.now() + 20000
  const params = { steps: [{ id: 'export.presets.save', params: { name: 'Saved', format: 'wav' } }, { id: 'unknown.command' }], stop_on_error: false }
  expect(await call('command_batch', params, { token: 'owner', expiresAt })).toBe(result)
  expect(request).toHaveBeenCalledExactlyOnceWith('owner', 'command.batch', params, expect.any(AbortSignal), expiresAt)
  expect(agentActivity.activities.find(item => item.tool === 'command_batch')).toMatchObject({ state: 'error', historyIds: [] })
  await expect(call('command_run', { id: 'command.batch', params })).rejects.toThrow('Use command_batch')
  expect(request).toHaveBeenCalledTimes(1)
})
it('routes project inspection as a read with exact serialized source snapshot', async () => {
  const document = '{"items":{"18446744073709551615":{"id":18446744073709551615}}}'
  const result = { name: 'Source', root: { id: '18446744073709551615', children: [] } }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: 'project.inspect', params: { document } })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', 'project.inspect', { document }, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(0)
})
it('classifies sequence inspection as a read and retains exact context IDs', async () => {
  const params = { document: '{}', item: '18446744073709551615', context: { selection: ['9007199254740993'] } }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ id: params.item, selection: params.context.selection })
  await call('command_run', { id: 'sequence.inspect', params })
  expect(request).toHaveBeenCalledWith('owner', 'sequence.inspect', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(0)
})
it('routes source monitor inspection as a read with exact playhead context', async () => {
  const params = { document: '{}', context: { sourceItem: '18446744073709551615', sourcePlayhead: '9007199254740993' } }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ item: params.context.sourceItem, playhead: params.context.sourcePlayhead })
  await call('command_run', { id: 'source.inspect', params })
  expect(request).toHaveBeenCalledWith('owner', 'source.inspect', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(0)
})
it('routes detailed effect catalog queries as reads without composition Undo', async () => {
  const params = { kind: 'VideoTransition', folder: 'Video Transitions/Wipe', detail: true }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue([{ id: 'wipe', paramInfo: [] }])
  await call('command_run', { id: 'effects.list', params })
  expect(request).toHaveBeenCalledWith('owner', 'effects.list', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(0)
})
it('returns authored effect snapshots as operations without silently replacing the open composition', async () => {
  const params = { document: '{}', sequence: '21', clip: '29', index: 0 }
  const result = { result: null, documentText: '{"changed":true}' }
  const before = JSON.stringify(compositionState.document)
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: 'effects.reset', params })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', 'effects.reset', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('operation')
  expect(JSON.stringify(compositionState.document)).toBe(before)
})
it('routes source effect application as a snapshot operation with exact clip selection', async () => {
  const params = { document: '{}', sequence: '18446744073709551615', effect: 'Gaussian Blur', clips: ['9007199254740993'] }
  const result = { result: null, documentText: '{"applied":true}' }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: 'effects.apply', params })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', 'effects.apply', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('operation')
})
it('routes original setParam payloads without lowering source clocks or parameter values', async () => {
  const params = { document: '{}', sequence: '18446744073709551615', clip: '9007199254740993', effect: 'motion', param: 'position', value: [17.25, -9.5], time: '9007199254740994', merge: true, begin: true }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null, documentText: '{}' })
  await call('command_run', { id: 'effects.setParam', params })
  expect(request).toHaveBeenCalledWith('owner', 'effects.setParam', params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('operation')
})
it.each(['effects.toggleAnimation', 'effects.addKeyframe', 'effects.deleteKeyframe', 'effects.moveKeyframe', 'effects.setInterpolation', 'effects.setKeyframe'] as const)('forwards %s with exact media clocks as a snapshot operation', async command => {
  const params = { document: '{}', sequence: '18446744073709551615', clip: '9007199254740993', effect: 2, mask: 0, param: 'opacity', mediaTime: '9007199254740994', to: '9007199254740995', interpolation: 'continuousBezier', value: 75, inInfluence: 0.4 }
  const result = { result: null, documentText: '{"keyEdited":true}' }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: command, params })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe('operation')
})
it.each(['masks.add', 'masks.remove', 'masks.set', 'masks.moveVertex', 'masks.translate', 'masks.addVertex', 'masks.removeVertex', 'masks.toggleVertexSmooth', 'masks.list', 'masks.select'] as const)('forwards %s with explicit exact source mask context', async command => {
  const params = { document: '{}', sequence: '18446744073709551615', context: { playhead: '9007199254740994', selectedMask: { clip: '9007199254740993', effect: 2, mask: 0 } }, delta: [0.1, -0.2] }
  const result = command === 'masks.list' ? { masks: [] } : { result: null, documentText: '{}', context: { selectedMask: null } }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: command, params })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe(command === 'masks.list' ? 'read' : 'operation')
})
it.each(['masks.track', 'jobs.list', 'jobs.cancel'] as const)('routes %s with owner-scoped native job controls', async command => {
  const params = command === 'masks.track' ? { document: '{}', sequence: '21', clip: '29', effect: 'opacity', mask: 0, sources: { '1': 'library-source' }, frames: 4, wait: false } : { job: '9007199254740993', snapshot: command === 'jobs.list' }
  const result = command === 'jobs.list' ? [{ id: '9007199254740993', finished: true, documentText: '{}' }] : command === 'jobs.cancel' ? null : { job: 1, frames: 4, status: { finished: false } }
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue(result)
  expect(await call('command_run', { id: command, params })).toEqual(result)
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.at(-1)?.kind).toBe(command === 'jobs.list' ? 'read' : 'operation')
})

it('routes clip.rename with exact item IDs and explicit source selection', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null, documentText: '{}' })
  const params = { document: '{}', item: '18446744073709551615', name: 'новое 🎬', context: { activeSequence: '9007199254740993', selection: ['9007199254740995'] } }
  await call('command_run', { id: 'clip.rename', params })
  expect(request).toHaveBeenCalledWith('owner', 'clip.rename', params, expect.any(AbortSignal))
})
it('routes project.renameBin with exact IDs and original whitespace', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: { bin: '18446744073709551615', name: 'Папка' }, documentText: '{}' })
  const params = { document: '{}', bin: '18446744073709551615', name: '  Папка  ' }
  await call('command_run', { id: 'project.renameBin', params })
  expect(request).toHaveBeenCalledWith('owner', 'project.renameBin', params, expect.any(AbortSignal))
})
it.each(['file.newBin', 'file.newBinFromSelection', 'project.moveToBin'] as const)('routes %s with exact source ordering and owner transport', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: { moved: 2 }, documentText: '{}' })
  const params = { document: '{}', items: ['18446744073709551615', '9007199254740993'], bin: '9007199254740995' }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
})
it.each(['edit.label', 'edit.label.rose', 'edit.label.magenta'] as const)('routes %s with exact selections and presence semantics', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ documentText: '{}', result: null })
  const params = { document: '{}', label: 'rOsE', items: null, clips: ['18446744073709551615'], context: { selection: ['9007199254740993'] } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
})
it('records label group as a read without composition history changes', async () => {
  const before = nestedHistory.past.length
  const ordinary = JSON.stringify(compositionState.document); const nested = JSON.stringify(nestedComposition.document)
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: { items: ['18446744073709551615'] } })
  const params = { document: '{}', context: { selection: [], projectSelection: ['18446744073709551615'] } }
  await call('command_run', { id: 'edit.selectLabelGroup', params })
  expect(request).toHaveBeenCalledWith('owner', 'edit.selectLabelGroup', params, expect.any(AbortSignal))
  expect(nestedHistory.past).toHaveLength(before)
  expect(JSON.stringify(compositionState.document)).toBe(ordinary); expect(JSON.stringify(nestedComposition.document)).toBe(nested)
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('read')
})
it.each(['edit.selectAllMatching', 'project.selectAll', 'project.deselectAll', 'project.select'] as const)('records %s as a context read without document/history changes', async command => {
  const before = nestedHistory.past.length; const ordinary = JSON.stringify(compositionState.document); const nested = JSON.stringify(nestedComposition.document)
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null, context: { selection: [], projectSelection: ['18446744073709551615'] } })
  const params = { document: '{}', context: { selection: [], projectSelection: ['18446744073709551615'] } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(before); expect(JSON.stringify(compositionState.document)).toBe(ordinary); expect(JSON.stringify(nestedComposition.document)).toBe(nested)
})
it('routes edit.duplicate as an operation with exact ordered context IDs', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: { items: ['18446744073709551614'] }, documentText: '{}' })
  const params = { document: '{}', context: { projectSelection: ['18446744073709551615', '9007199254740993', '18446744073709551615'] } }
  await call('command_run', { id: 'edit.duplicate', params })
  expect(request).toHaveBeenCalledWith('owner', 'edit.duplicate', params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it('routes project.delete with exact folder IDs and session context as an operation', async () => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: { items: 3, bins: 1 }, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', items: ['18446744073709551615'], context: { activeSequence: '9007199254740993', projectSelection: ['18446744073709551615'], selection: ['9007199254740995'] } }
  await call('command_run', { id: 'project.delete', params })
  expect(request).toHaveBeenCalledWith('owner', 'project.delete', params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['project.setMarks', 'markers.markSplitAudioOut'] as const)('routes %s with exact ticks and one native request', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null, markEdit: true, documentText: '{}' })
  const params = { document: '{}', item: '18446744073709551615', in: '-9223372036854775808', out: null, target: 'source', time: '9007199254740997', context: { sourceItem: '18446744073709551615' } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})
it.each(['source.open', 'source.setPlayhead', 'markers.goToIn', 'markers.goToOut', 'markers.goToSplitVideoIn', 'markers.goToSplitVideoOut', 'markers.goToSplitAudioIn', 'markers.goToSplitAudioOut'] as const)('routes %s as context navigation without composition history changes', async command => {
  const before = nestedHistory.past.length
  const ordinary = JSON.stringify(compositionState.document); const nested = JSON.stringify(nestedComposition.document)
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null, context: {} })
  const params = { document: '{}', target: 'source', time: '9007199254740997', context: { sourceItem: '18446744073709551615' } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('read')
  expect(nestedHistory.past).toHaveLength(before); expect(JSON.stringify(compositionState.document)).toBe(ordinary); expect(JSON.stringify(nestedComposition.document)).toBe(nested)
})
it.each(['source.insert', 'source.overwrite'] as const)('routes %s as one authoring operation with exact source patch context', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ sourceEdit: true, documentText: '{}' })
  const params = { document: '{}', context: { activeSequence: '18446744073709551615', sourceItem: '9007199254740993', playhead: '9007199254740997', targeting: { videoDest: '9007199254740995', audioDest: null }, sequencesAsClips: true } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['clip.makeSubclip', 'clip.editSubclip', 'clip.subclipDefaults'] as const)('routes %s with exact boundaries and the correct activity class', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ result: null })
  const params = { document: '{}', item: '18446744073709551615', start: '9007199254740997', end: '9007199254740999', context: { projectSelection: [] } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe(command === 'clip.subclipDefaults' ? 'read' : 'operation')
})

it.each(['edit.pasteAttributes', 'edit.removeAttributes'] as const)('routes %s as one operation without parsing clipboard text', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ attributeEdit: true, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', clips: ['9007199254740993'], clipboardText: 'opaque', effects: ['brightness_contrast'], scaleTimes: false, context: { activeSequence: '18446744073709551615', selection: [], linkedSelection: true } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['clip.replaceFromSource', 'clip.replaceFromSourceMatchFrame', 'clip.replaceFromBin'] as const)('routes %s with exact identities and monitor clocks', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ replacementEdit: true, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', clips: ['9007199254740993'], item: '9007199254740995', keepSourceIn: true, context: { activeSequence: '18446744073709551615', sourceItem: '9007199254740995', sourcePlayhead: '9007199254740997', playhead: '9007199254740999', selection: [], projectSelection: [], linkedSelection: true } }
  await call('command_run', { id: command, params })
  expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal))
  expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['clip.frameHoldOptions', 'clip.fieldOptions', 'clip.timeInterpolation.frameSampling', 'clip.timeInterpolation.frameBlending', 'clip.timeInterpolation.opticalFlow', 'clip.setTimeInterpolation', 'clip.fitToFrame', 'clip.fillFrame'] as const)('routes %s as an exact source-video operation', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ videoOptionsEdit: true, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', clips: ['9007199254740993'], holdOn: 'sourceTimecode', time: '9007199254740997', mode: 'opticalFlow', context: { activeSequence: '18446744073709551615', selection: [], playhead: '9007199254740999' } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['clip.audioChannels', 'clip.modifyTimecode', 'clip.breakoutToMono'] as const)('routes %s as one source media modification', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ modifyEdit: true, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', items: ['9007199254740993'], frame: '9007199254740997', format: 'mono', context: { activeSequence: '18446744073709551615', projectSelection: [], selection: [] } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['clip.speedDuration', 'clip.enable', 'clip.link', 'clip.group', 'clip.ungroup', 'clip.scaleToFrameSize', 'clip.frameHold', 'clip.insertFrameHoldSegment'] as const)('routes %s as one timeline edit', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ clipEdit: true, documentText: '{}' })
  const params = { document: '{"id":18446744073709551615}', clips: ['9007199254740993'], speed: 50, ripple: true, time: '9007199254740997', context: { activeSequence: '18446744073709551615', selection: [], linkedSelection: true } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('operation')
})

it.each(['sequence.matchFrame', 'clip.revealInProject', 'sequence.revealInProject'] as const)('routes %s as context navigation without document history', async command => {
  const request = vi.spyOn(api, 'runFilmcraftExportCommand').mockResolvedValue({ context: {}, events: [] })
  const params = { document: '{"id":18446744073709551615}', context: { activeSequence: '18446744073709551615', sourcePlayhead: '9007199254740997', playhead: '9007199254740999', selection: [], projectSelection: [] } }
  await call('command_run', { id: command, params }); expect(request).toHaveBeenCalledWith('owner', command, params, expect.any(AbortSignal)); expect(agentActivity.activities.findLast(activity => activity.tool === 'command_run')?.kind).toBe('read')
})
