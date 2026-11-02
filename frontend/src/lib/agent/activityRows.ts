import type {AgentActivity} from './activity.svelte'
export type ActivityRow={kind:'activity';id:string;activity:AgentActivity}|{kind:'reads';id:string;activities:AgentActivity[]}
export interface ActivitySection {id:string;sessionId:string|null;rows:ActivityRow[]}
const quick=(item:AgentActivity)=>item.kind==='read'&&item.state==='done'&&(item.durationMs??Infinity)<250
/** Consecutive sections preserve interleaved client chronology; only adjacent reads of one actor fold. */
export function activitySections(items:readonly AgentActivity[]):ActivitySection[]{
 const result:ActivitySection[]=[]
 for(const item of items){
  let section=result.at(-1)
  if(!section||section.sessionId!==item.sessionId){section={id:item.id,sessionId:item.sessionId,rows:[]};result.push(section)}
  const previous=section.rows.at(-1),last=previous?.kind==='reads'?previous.activities.at(-1):previous?.activity
  if(last&&quick(last)&&quick(item)&&last.actor===item.actor){
   if(previous!.kind==='reads')previous!.activities.push(item)
   else section.rows[section.rows.length-1]={kind:'reads',id:last.id,activities:[last,item]}
  }else section.rows.push({kind:'activity',id:item.id,activity:item})
 }
 return result
}
