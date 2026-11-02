/** Runtime identities belong to actual history entries, never stack depths or document hashes. */
export type HistoryEffect='applied'|'reverted'|'partial'|'unknown'|null
const identities=new WeakMap<object,string>()
const providers=new Map<string,()=>{past:readonly object[];future:readonly object[]}>()
export function registerHistoryProvider(scope:string,provider:()=>{past:readonly object[];future:readonly object[]}){providers.set(scope,provider);return ()=>{if(providers.get(scope)===provider)providers.delete(scope)}}
export function identifyHistoryEntry(entry:object,id?:string):string {const identity=id??identities.get(entry)??crypto.randomUUID();identities.set(entry,identity);return identity}
export function historyEntryId(entry:object):string|undefined{return identities.get(entry)}
export function transferHistoryEntry(from:object,to:object){const id=identities.get(from);if(id)identities.set(to,id)}
export function historyEffect(ids:readonly string[]):HistoryEffect {
 if(!ids.length)return null
 const applied=new Set<string>(),reverted=new Set<string>()
 for(const provider of providers.values()){
  const stacks=provider()
  for(const item of stacks.past){const id=identities.get(item);if(id)applied.add(id)}
  for(const item of stacks.future){const id=identities.get(item);if(id)reverted.add(id)}
 }
 if(ids.some(id=>!applied.has(id)&&!reverted.has(id)))return 'unknown'
 const count=ids.filter(id=>applied.has(id)).length
 return count===ids.length?'applied':count===0?'reverted':'partial'
}
