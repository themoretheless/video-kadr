import { audioRoleGain, audioRoleMix } from '../composition/audioRoles'
import type { Composition, CompositionAudioRole } from '../composition/types'
import { setPreviewMasterMuted } from './mediaGain'
export const previewAudioState = $state<{ muted: boolean; roleGains: Partial<Record<CompositionAudioRole, number>> }>({ muted: false, roleGains: {} })
export function setPreviewAudioMuted(muted: boolean): void {
  previewAudioState.muted = muted
  setPreviewMasterMuted(muted)
}

export function setPreviewRoleGain(role: CompositionAudioRole, gainDb?: number): void {
  if (gainDb === undefined) delete previewAudioState.roleGains[role]
  else if (Number.isFinite(gainDb)) previewAudioState.roleGains[role] = Math.max(-30, Math.min(20, gainDb))
}
export function clearPreviewRoleGains(): void { previewAudioState.roleGains = {} }
export function previewAudioRoleGain(composition: Composition, role: CompositionAudioRole = 'dialogue'): number {
  const gainDb = previewAudioState.roleGains[role]
  if (gainDb === undefined) return audioRoleGain(composition, role)
  return audioRoleGain({ ...composition, audioRoles: { ...composition.audioRoles, [role]: { ...audioRoleMix(composition, role), gainDb } } }, role)
}
