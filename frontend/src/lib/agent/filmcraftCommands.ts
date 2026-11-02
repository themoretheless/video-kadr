import { runFilmcraftExportCommand, type FilmcraftNativeCommand } from '../api'
import type { AgentAccess } from './runtime.svelte'

// JSON.parse cannot retain source u64 IDs or long timeline ticks as JS numbers.
// Source snapshots may be serialized strings; exact IDs/ticks use decimal strings.
function assertExactNumbers(value: unknown): void {
  if (typeof value === 'number' && (!Number.isFinite(value) || Number.isInteger(value) && !Number.isSafeInteger(value))) throw new Error('Use decimal strings for large source IDs/ticks and a serialized document snapshot')
  if (Array.isArray(value)) value.forEach(assertExactNumbers)
  else if (value && typeof value === 'object') Object.values(value).forEach(assertExactNumbers)
}

export async function executeFilmcraftCommand(actor: string, tool: string, args: Record<string, unknown>, access: AgentAccess): Promise<unknown> {
  if (!actor || actor.length > 128) throw new Error('Invalid actor')
  if (!access.token) throw new Error('Filmcraft commands require an authenticated owner')
  const live = () => (!access.isActive || access.isActive()) && (access.expiresAt === undefined || Date.now() < access.expiresAt)
  if (!live()) throw new Error('Agent request expired or disconnected')
  const allowed = tool === 'command_list' ? ['filter', 'enabled_only'] : tool === 'command_batch' ? ['steps', 'stop_on_error'] : ['id', 'params']
  if (Object.keys(args).some(key => !allowed.includes(key))) throw new Error('Unknown command tool argument')
  if (tool === 'command_run' && (typeof args.id !== 'string' || !args.id)) throw new Error('Command id is required')
  if (tool === 'command_run' && args.id === 'command.batch') throw new Error('Use command_batch for batched commands')
  const params = tool === 'command_list' || tool === 'command_batch' ? args : args.params ?? {}
  if (!params || typeof params !== 'object' || Array.isArray(params)) throw new Error('Command params must be an object')
  assertExactNumbers(params)
  const controller = new AbortController()
  const timer = access.isActive || access.expiresAt !== undefined ? setInterval(() => { if (!live()) controller.abort() }, 100) : undefined
  try {
    const command = (tool === 'command_list' ? 'command.list' : tool === 'command_batch' ? 'command.batch' : args.id) as FilmcraftNativeCommand
    const result = access.expiresAt === undefined
      ? await runFilmcraftExportCommand(access.token, command, params as Record<string, unknown>, controller.signal)
      : await runFilmcraftExportCommand(access.token, command, params as Record<string, unknown>, controller.signal, access.expiresAt)
    if (!live()) throw new Error('Command completion unconfirmed; inspect queue/presets before retrying')
    return result
  } catch (failure) {
    if (!live()) throw new Error('Command completion unconfirmed; inspect queue/presets before retrying')
    throw failure
  } finally { clearInterval(timer) }
}
