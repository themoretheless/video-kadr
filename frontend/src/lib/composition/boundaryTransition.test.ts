import {expect,test} from 'vitest'
import {boundaryTransition} from './boundaryTransition'
import type {CompositionTransition} from './types'

test('inspector update preserves source-tick interval and reverse unless duration changes',()=>{
 const existing:CompositionTransition={id:'fade',kind:'film_dissolve',fromClipId:'a',toClipId:'b',durationTicks:0,reverse:true,timeline:{startTick:'-1',durationTick:'254016000001'}}
 const same=boundaryTransition('fade','a','b','additive_dissolve',1000000,4000000,existing)
 expect(same).toMatchObject({kind:'additive_dissolve',durationTicks:0,reverse:true,timeline:existing.timeline})
 const longer=boundaryTransition('fade','a','b','fade_white',1500000,4000000,existing)
 expect(longer.timeline).toEqual({startTick:'-1',durationTick:1500000})
 expect(existing.timeline).toEqual({startTick:'-1',durationTick:'254016000001'})
})

test('independent authoring accepts signed start and explicit reverse on an existing legacy kind',()=>{
 expect(boundaryTransition('fade','a','b','dissolve',500000,4000000,undefined,{independent:true,startTick:'-1',reverse:true})).toEqual({id:'fade',fromClipId:'a',toClipId:'b',kind:'dissolve',durationTicks:0,timeline:{startTick:'-1',durationTick:500000},reverse:true})
})
