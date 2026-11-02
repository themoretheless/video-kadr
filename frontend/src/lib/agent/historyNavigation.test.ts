import {expect,it} from 'vitest'
import {describeHistoryStacks,planHistoryJump} from './historyNavigation'
import {transferHistoryEntry,identifyHistoryEntry} from './historyProvenance'
it('lists retained transitions in chronological order with stable IDs across Undo/Redo',()=>{
 const a={},b={},c={},history=describeHistoryStacks([a,b],[c])
 expect(history.cursor).toBe(2);expect(history.entries.map(entry=>entry.state)).toEqual(['applied','applied','reverted'])
 expect(describeHistoryStacks([a,b],[c]).entries).toEqual(history.entries)
 expect(identifyHistoryEntry(a)).toBe(history.entries[0]!.id)
 const restored={};transferHistoryEntry(b,restored)
 expect(describeHistoryStacks([a],[c,restored]).entries.map(entry=>entry.id)).toEqual(history.entries.map(entry=>entry.id))
 expect(planHistoryJump([a,b],[c],history.entries[0]!.id)).toEqual({direction:'undo',steps:1})
 expect(planHistoryJump([a],[c,restored],history.entries[2]!.id)).toEqual({direction:'redo',steps:2})
 expect(planHistoryJump([a,b],[c],history.entries[0]!.id,'before')).toEqual({direction:'undo',steps:2})
 expect(()=>planHistoryJump([a,b],[],history.entries[2]!.id)).toThrow('no longer')
})
