// Aggregator over the six feature store modules. It is the single place the
// rest of the app touches them: undo/redo snapshots, project autosave/restore
// and "open a new clip" all go through here, so a feature agent only ever has
// to keep its own module's four contract symbols correct.

import { cloneValue } from '../domain/history'
import {
  applyAudioMixSnapshot,
  audioMixPayload,
  audioMixState,
  resetAudioMix,
} from './audioMix'
import {
  applyColorAdvancedSnapshot,
  colorAdvancedPayload,
  colorAdvancedState,
  resetColorAdvanced,
} from './colorAdvanced'
import {
  applyCompositionSnapshot,
  compositionPayload,
  compositionState,
  resetComposition,
} from './composition'
import { applyMotionSnapshot, motionPayload, motionState, resetMotion } from './motion'
import {
  applyOverlaysSnapshot,
  overlaysPayload,
  overlaysState,
  resetOverlays,
} from './overlays'
import { applySpatialSnapshot, resetSpatial, spatialPayload, spatialState } from './spatial'
import { isRecord } from './validation'

/** Persisted/undoable snapshot of every feature module, keyed by module name. */
export interface FeatureModuleSnapshot {
  composition: unknown
  overlays: unknown
  audioMix: unknown
  motion: unknown
  spatial: unknown
  colorAdvanced: unknown
}

export const FEATURE_MODULE_KEYS = [
  'composition',
  'overlays',
  'audioMix',
  'motion',
  'spatial',
  'colorAdvanced',
] as const

export type FeatureModuleKey = (typeof FEATURE_MODULE_KEYS)[number]

/** Live reactive states, for deep watchers that must see any module change. */
export function featureModuleStates(): unknown[] {
  return [
    compositionState,
    overlaysState,
    audioMixState,
    motionState,
    spatialState,
    colorAdvancedState,
  ]
}

/** Detached deep copy: later Vue mutations cannot rewrite a stored snapshot. */
export function captureFeatureModules(): FeatureModuleSnapshot {
  return {
    composition: cloneValue(compositionState),
    overlays: cloneValue(overlaysState),
    audioMix: cloneValue(audioMixState),
    motion: cloneValue(motionState),
    spatial: cloneValue(spatialState),
    colorAdvanced: cloneValue(colorAdvancedState),
  }
}

/** Restore from untrusted JSON. Every module validates before assigning. */
export function applyFeatureModules(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  applyCompositionSnapshot(source.composition)
  applyOverlaysSnapshot(source.overlays)
  applyAudioMixSnapshot(source.audioMix)
  applyMotionSnapshot(source.motion)
  applySpatialSnapshot(source.spatial)
  applyColorAdvancedSnapshot(source.colorAdvanced)
}

/**
 * Merged wire contribution of every module. Modules return `{}` while they sit
 * at their defaults, so a default project produces today's exact payload.
 * `buildEditPayload` takes this as an argument: domain code must not reach into
 * the store, so the merge lives on this side of the boundary.
 */
export function featureModulePayload(): Record<string, unknown> {
  return Object.assign(
    {},
    compositionPayload(),
    overlaysPayload(),
    audioMixPayload(),
    motionPayload(),
    spatialPayload(),
    colorAdvancedPayload(),
  )
}

/** True when at least one module would contribute something to the wire. */
export function featureModulesActive(): boolean {
  return Object.keys(featureModulePayload()).length > 0
}

export function resetFeatureModules(): void {
  resetComposition()
  resetOverlays()
  resetAudioMix()
  resetMotion()
  resetSpatial()
  resetColorAdvanced()
}
