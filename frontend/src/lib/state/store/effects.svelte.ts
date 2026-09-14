import * as core from './core.svelte.js'
import {
  armHistoryDebounceTimer,
  clearHistoryDebounceTimer,
  historyTransaction
} from './history.svelte.js'

let disposeStateEffects: (() => void) | null = null

export function initStateEffects(): () => void {
  if (disposeStateEffects) return disposeStateEffects

  const disposeRoot = $effect.root(() => {
    let editEffectReady = false
    $effect(() => {
      JSON.stringify(core.state.edit)
      if (!editEffectReady) {
        editEffectReady = true
        return
      }

      core.bumpEditRevision()
      if (historyTransaction) return
      armHistoryDebounceTimer()
    })

    let autosaveEffectReady = false
    $effect(() => {
      const videoId = core.state.video?.id ?? null
      JSON.stringify(core.state.edit)
      if (!autosaveEffectReady) {
        autosaveEffectReady = true
        return
      }
      if (!videoId) {
        core.clearProjectSaveTimer()
        return
      }
      if (core.restoringProjectFor === videoId) {
        core.clearProjectSaveTimer()
        return
      }
      if (core.restoredProjectFor === videoId) {
        core.clearRestoredProjectFor()
        core.clearProjectSaveTimer()
        return
      }
      core.scheduleProjectSave()
    })

    return () => {
      clearHistoryDebounceTimer()
      core.clearProjectSaveTimer()
    }
  })

  disposeStateEffects = () => {
    disposeStateEffects = null
    disposeRoot()
  }
  return disposeStateEffects
}
