import { COMPOSITION_AUDIO_ROLES, type Composition, type CompositionAudioRole, type CompositionRoleMix } from './types'
export function audioRoleMix(composition: Composition, role: CompositionAudioRole): CompositionRoleMix {
  return composition.audioRoles?.[role] ?? { gainDb: 0, muted: false, solo: false }
}
export function audioRoleGain(composition: Composition, role: CompositionAudioRole = 'dialogue'): number {
  const mix = audioRoleMix(composition, role)
  return mix.muted || audioRoleSilencedBySolo(composition, role) ? 0 : 10 ** (mix.gainDb / 20)
}

export function audioRoleSilencedBySolo(composition: Composition, role: CompositionAudioRole): boolean {
  const mix = audioRoleMix(composition, role)
  return !mix.muted && !mix.solo && COMPOSITION_AUDIO_ROLES.some(candidate => audioRoleMix(composition, candidate).solo)
}
