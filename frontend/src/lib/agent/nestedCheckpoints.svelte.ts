import type {NativeNestedProject} from '../api'
import {nestedAgentGeneration,nestedComposition} from '../state/nestedComposition.svelte'
export interface NestedCheckpointSummary {id:string;label:string;actor:string;createdAt:number}
export const nestedCheckpointState=$state<{generation:number;items:NestedCheckpointSummary[]}>({generation:-1,items:[]})
let documents:Record<string,NativeNestedProject>=Object.create(null)
function context(){
 const generation=nestedAgentGeneration()
 if(nestedCheckpointState.generation!==generation){documents=Object.create(null);nestedCheckpointState.generation=generation;nestedCheckpointState.items=[]}
}
export function visibleNestedCheckpoints():readonly NestedCheckpointSummary[]{
 return nestedComposition.document&&nestedCheckpointState.generation===nestedAgentGeneration()?nestedCheckpointState.items:[]
}
export function listNestedCheckpoints(){context();return nestedCheckpointState.items.map(item=>({...item}))}
export function createNestedCheckpoint(label:string,actor:string,document:NativeNestedProject){
 context()
 if(!label.trim()||label.length>128)throw new Error('Invalid checkpoint label')
 if(nestedCheckpointState.items.length>=20)throw new Error('Checkpoint limit is 20; delete a checkpoint first')
 const summary={id:`checkpoint-${crypto.randomUUID()}`,label,actor,createdAt:Date.now()}
 documents[summary.id]=JSON.parse(JSON.stringify(document))
 nestedCheckpointState.items=[...nestedCheckpointState.items,summary]
 return {...summary}
}
export function readNestedCheckpoint(id:string):NativeNestedProject{
 context()
 if(!Object.hasOwn(documents,id))throw new Error('Checkpoint not found')
 return JSON.parse(JSON.stringify(documents[id])) as NativeNestedProject
}
export function deleteNestedCheckpoint(id:string){
 context()
 if(!Object.hasOwn(documents,id))throw new Error('Checkpoint not found')
 delete documents[id];nestedCheckpointState.items=nestedCheckpointState.items.filter(item=>item.id!==id)
}
