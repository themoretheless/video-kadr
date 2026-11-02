import type {SourceRunClipboard} from './sourceRunClipboard'
import {sourceRunClipboard} from '$lib/state/sourceRunClipboard.svelte'
export interface SourceRunCutTransaction {
 claimed:boolean
 result:Promise<boolean>
 validate:()=>void
 settle:(committed:boolean)=>void
}
let active:SourceRunCutTransaction|null=null
/** Synchronous handoff to the native edit entry; completion outlives the editor component. */
export function createSourceRunCutTransaction(clipboard:SourceRunClipboard,validate:()=>void):SourceRunCutTransaction{
 const previous=sourceRunClipboard.value
 let settled=false,finish!:(committed:boolean)=>void
 const result=new Promise<boolean>(resolve=>{finish=resolve})
 return {claimed:false,result,validate,settle(committed){if(settled)return;settled=true;if(committed&&sourceRunClipboard.value===previous)sourceRunClipboard.value=clipboard;finish(committed)}}
}
export function withSourceRunCutTransaction(transaction:SourceRunCutTransaction,action:()=>void):void{
 const previous=active;active=transaction
 try{action()}finally{active=previous}
}
export function takeSourceRunCutTransaction():SourceRunCutTransaction|null{
 const transaction=active
 if(!transaction||transaction.claimed)return null
 transaction.claimed=true
 return transaction
}
