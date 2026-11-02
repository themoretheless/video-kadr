/** WeftCut fade_multiplier: overlapping linear fades multiply. */
export function audioFadeMultiplier(localTicks: number, durationTicks: number, fadeInTicks = 0, fadeOutTicks = 0): number {
  const fadeIn = fadeInTicks > 0 ? Math.max(0, Math.min(1, localTicks / fadeInTicks)) : 1
  const fadeOut = fadeOutTicks > 0 ? Math.max(0, Math.min(1, (durationTicks - localTicks) / fadeOutTicks)) : 1
  return fadeIn * fadeOut
}

/** Browser monitor envelope; sample-accurate exported PCM is evaluated by Rust/FFmpeg. */
export function audioCrossfadeMultiplier(progress: number, incoming: boolean, curve: 'constant_gain' | 'constant_power' | 'exponential_fade' = 'constant_gain'): number {
  const p = Math.max(0, Math.min(1, progress))
  if (curve === 'constant_power') return incoming ? Math.sin(p * Math.PI / 2) : Math.cos(p * Math.PI / 2)
  if (curve === 'exponential_fade') return incoming ? 1 - (1 - p) ** 3 : (1 - p) ** 3
  return incoming ? p : 1 - p
}
