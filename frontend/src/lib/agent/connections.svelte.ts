import {activityError} from './activityDetails'
export interface AgentConnection {id:string;client:string;version:string;connectedAt:number;lastActivityAt:number}
export const agentConnections=$state<{items:readonly AgentConnection[]}>({items:[]})
export function updateAgentConnections(value:unknown){
 if(!Array.isArray(value))return
 const items:AgentConnection[]=[]
 for(const item of value.slice(0,128)){
  if(!item||typeof item!=='object'||typeof item.id!=='string'||item.id.length>128||typeof item.client!=='string'||typeof item.version!=='string'||!Number.isSafeInteger(item.connectedAt)||!Number.isSafeInteger(item.lastActivityAt))continue
  if(items.some(known=>known.id===item.id))continue
  items.push({id:item.id,client:activityError(item.client).slice(0,128),version:activityError(item.version).slice(0,64),connectedAt:item.connectedAt,lastActivityAt:item.lastActivityAt})
 }
 agentConnections.items=items
}
