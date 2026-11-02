import {expect,it} from 'vitest'
import {historyEffect,identifyHistoryEntry,transferHistoryEntry,registerHistoryProvider} from './historyProvenance'
it('uses entry identity across transfers and reports partial and unknown without comparing snapshots',()=>{
 const first={},second={},sameContent={}
 const id=identifyHistoryEntry(first),other=identifyHistoryEntry(second)
 const stacks={past:[first,second],future:[] as object[]}
 const dispose=registerHistoryProvider('test',()=>stacks)
 try{
  expect(historyEffect([])).toBeNull()
  expect(historyEffect([id,other])).toBe('applied')
  stacks.past=[second];stacks.future=[sameContent]
  expect(historyEffect([id])).toBe('unknown')
  transferHistoryEntry(first,sameContent)
  expect(historyEffect([id])).toBe('reverted')
  expect(historyEffect([id,other])).toBe('partial')
  stacks.future=[]
  expect(historyEffect([id,other])).toBe('unknown')
 }finally{dispose()}
})
