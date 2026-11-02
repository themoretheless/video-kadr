import {expect,it} from 'vitest'
import {nativeTransitionSpan} from './nativeTransitionSpan'
import {CAPTION_SOURCE_TICKS_PER_MICROSECOND as unit} from './captionTime'
import type {WireClipTransition} from './types'
const transition=(start:bigint,duration:bigint):WireClipTransition=>({id:'fade',fromClipId:null,toClipId:null,kind:'dissolve',durationTicks:0,timeline:{startTick:String(start),durationTick:String(duration)}})
it('rounds exact transition windows outward including negative starts and single source ticks',()=>{
 expect(nativeTransitionSpan(transition(2n*unit+1n,unit))).toEqual([2,4])
 expect(nativeTransitionSpan(transition(-1n,2n))).toEqual([-1,1])
 expect(nativeTransitionSpan(transition(-2n*unit-1n,unit))).toEqual([-3,-1])
 expect(nativeTransitionSpan(transition(0n,1n))).toEqual([0,1])
 expect(()=>nativeTransitionSpan(transition(0n,0n))).toThrow('время перехода')
})
