import {CAPTION_SOURCE_TICKS_PER_MICROSECOND} from './captionTime'
import type {WireClipTransition} from './types'
/** Outward rounding keeps every exact source tick inside the placement envelope. */
export function nativeTransitionSpan(transition:WireClipTransition):readonly[number,number]|null{
 if(!transition.timeline)return null
 try{
  const unit=CAPTION_SOURCE_TICKS_PER_MICROSECOND,a=BigInt(transition.timeline.startTick),b=a+BigInt(transition.timeline.durationTick)
  const from=Number(a<0n?-((-a+unit-1n)/unit):a/unit),end=Number(b<0n?-((-b)/unit):(b+unit-1n)/unit)
  if(!Number.isSafeInteger(from)||!Number.isSafeInteger(end)||b<=a)throw Error()
  return [from,end]
 }catch{throw new Error('Некорректное время перехода')}
}
