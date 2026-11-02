import type {AudioClip,VideoClip} from './types'
import {cloneAudioAnimation} from './keyframes'
import {cloneDspEffect} from './dsp'
import {cloneGainSourceKeys} from './concatGainSourceKeys'
/** The embedded stream and detached clip share one clip-local processing clock. */
export function embeddedAudioMix(clip:VideoClip):Pick<AudioClip,'audioStream'|'gain'|'pan'|'audioAnimation'|'dspEffects'|'concatGainKeys'|'fadeInTicks'|'fadeOutTicks'>{
 return {audioStream:clip.audioStream,gain:clip.audioGain,pan:clip.audioPan??0,audioAnimation:cloneAudioAnimation(clip.audioAnimation),fadeInTicks:clip.audioFadeInTicks??0,fadeOutTicks:clip.audioFadeOutTicks??0,...(clip.dspEffects?{dspEffects:clip.dspEffects.map(cloneDspEffect)}:{}),...(clip.concatGainKeys?{concatGainKeys:cloneGainSourceKeys(clip.concatGainKeys)}:{})}
}
