import {
  cloneValue,
  PatchCommand
} from '../../domain/history.js'
import type {
  EditState
} from '../../types'
import {
  state
} from './core.svelte.js'

type EditCommand = PatchCommand<EditState>

export const history = $state({ past: [] as EditCommand[], future: [] as EditCommand[] })
let historyBaseline: EditState | undefined

function historyBaselineValue(): EditState {
  if (!historyBaseline) historyBaseline = cloneValue(state.edit)
  return historyBaseline
}
export let historyTimer: ReturnType<typeof setTimeout> | null = null
export let historyTransaction: { key: string; before: EditState; depth: number } | null = null

function commitHistory(command: EditCommand | null): void {
  if (!command) return
  history.past.push(command)
  if (history.past.length > 100) history.past.shift()
  history.future = []
}

export function recordChange(): void {
  historyTimer = null
  const current = cloneValue(state.edit)
  commitHistory(PatchCommand.between(historyBaselineValue(), current, 'debounced-edit'))
  historyBaseline = current
}

export function resetHistory(): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  history.past = []
  history.future = []
  historyTransaction = null
  historyBaseline = cloneValue(state.edit)
}

function applyHistory(command: EditCommand): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  state.edit = command.apply(state.edit)
  historyBaseline = cloneValue(state.edit)
}

export function flushPendingHistory(): void {
  if (historyTransaction) {
    historyTransaction.depth = 1
    endEditTransaction()
  }
  if (!historyTimer) return
  clearTimeout(historyTimer)
  recordChange()
}

export function undo(): void {
  flushPendingHistory()
  const command = history.past.pop()
  if (!command) return
  history.future.push(command)
  applyHistory(command.invert() as EditCommand)
}

export function redo(): void {
  flushPendingHistory()
  const command = history.future.pop()
  if (!command) return
  history.past.push(command)
  applyHistory(command)
}

export function beginEditTransaction(key: string): void {
  flushPendingHistory()
  if (historyTransaction) {
    historyTransaction.depth++
    return
  }
  historyTransaction = { key, before: cloneValue(state.edit), depth: 1 }
}

export function endEditTransaction(): void {
  const transaction = historyTransaction
  if (!transaction) return
  transaction.depth--
  if (transaction.depth > 0) return
  const current = cloneValue(state.edit)
  commitHistory(PatchCommand.between(transaction.before, current, transaction.key))
  historyBaseline = current
  historyTransaction = null
}

export function armHistoryDebounceTimer(): void {
  if (historyTimer) clearTimeout(historyTimer)
  historyTimer = setTimeout(recordChange, 350)
}

export function clearHistoryDebounceTimer(): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
}
