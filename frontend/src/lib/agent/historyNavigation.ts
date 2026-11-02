import {identifyHistoryEntry} from './historyProvenance'
export function describeHistoryStacks(past:readonly object[],future:readonly object[]){
 const entries=[...past,...[...future].reverse()].map((entry,index)=>({id:identifyHistoryEntry(entry),position:index+1,state:index<past.length?'applied' as const:'reverted' as const,current:index+1===past.length}))
 return {cursor:past.length,entries}
}
export function planHistoryJump(past:readonly object[],future:readonly object[],entryId:string,side:'before'|'after'='after'){
 if(side!=='before'&&side!=='after')throw new Error('Invalid history jump side')
 const history=describeHistoryStacks(past,future),entry=history.entries.find(entry=>entry.id===entryId)
 if(!entry)throw new Error('History entry is no longer available')
 const destination=entry.position-(side==='before'?1:0)
 return {direction:destination<history.cursor?'undo' as const:'redo' as const,steps:Math.abs(destination-history.cursor)}
}
