import {beforeEach,expect,it} from 'vitest'
import {createSourceRunCutTransaction,takeSourceRunCutTransaction,withSourceRunCutTransaction} from './sourceRunCutTransaction'
import {captureSourceRunClipboard} from './sourceRunClipboard'
import {sourceRunClipboard} from '$lib/state/sourceRunClipboard.svelte'
beforeEach(()=>{sourceRunClipboard.value=null})
it('hands off only within the synchronous action and publishes exactly once after acceptance',async()=>{
 const clipboard=captureSourceRunClipboard([{at:0,value:1}],[0]),transaction=createSourceRunCutTransaction(clipboard,()=>{})
 expect(takeSourceRunCutTransaction()).toBeNull();withSourceRunCutTransaction(transaction,()=>{expect(takeSourceRunCutTransaction()).toBe(transaction);expect(takeSourceRunCutTransaction()).toBeNull()});expect(takeSourceRunCutTransaction()).toBeNull();expect(sourceRunClipboard.value).toBeNull();transaction.settle(true);transaction.settle(false);expect(await transaction.result).toBe(true);expect(sourceRunClipboard.value).toBe(clipboard)
})
it('retains prior clipboard on refusal and does not overwrite a later explicit copy',async()=>{
 const old=captureSourceRunClipboard([{at:0,value:1}],[0]),cut=captureSourceRunClipboard([{at:0,value:2}],[0]),latest=captureSourceRunClipboard([{at:0,value:3}],[0]);sourceRunClipboard.value=old
 const refused=createSourceRunCutTransaction(cut,()=>{});refused.settle(false);expect(await refused.result).toBe(false);expect(sourceRunClipboard.value).toBe(old)
 const accepted=createSourceRunCutTransaction(cut,()=>{});sourceRunClipboard.value=latest;accepted.settle(true);expect(await accepted.result).toBe(true);expect(sourceRunClipboard.value).toBe(latest)
})
