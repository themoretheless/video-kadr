export interface ActivityAnchor {id:string;top:number;bottom:number}
export interface ActivityScrollMetrics {scrollTop:number;scrollHeight:number;clientHeight:number}
export interface ActivityViewport {following:boolean;anchors:{id:string;offset:number}[];scrollTop:number}
export const initialActivityViewport=():ActivityViewport=>({following:true,anchors:[],scrollTop:0})
/** Retain visible fallback anchors too, so removing the first row does not move the next one. */
export function captureActivityViewport(metrics:ActivityScrollMetrics,rows:readonly ActivityAnchor[]):ActivityViewport {
 return {following:metrics.scrollHeight-metrics.clientHeight-metrics.scrollTop<24,scrollTop:metrics.scrollTop,anchors:rows.filter(row=>row.bottom>0&&row.top<metrics.clientHeight).map(row=>({id:row.id,offset:row.top}))}
}
export function restoreActivityViewport(saved:ActivityViewport,metrics:ActivityScrollMetrics,rows:readonly ActivityAnchor[]):number {
 const max=Math.max(0,metrics.scrollHeight-metrics.clientHeight),clamp=(value:number)=>Math.max(0,Math.min(max,value))
 if(saved.following)return max
 for(const anchor of saved.anchors){const row=rows.find(row=>row.id===anchor.id);if(row)return clamp(metrics.scrollTop+row.top-anchor.offset)}
 return clamp(saved.scrollTop)
}
export function retainActivityExpansion(expanded:Readonly<Record<string,boolean>>,ids:readonly string[]):Record<string,boolean>{
 const valid=new Set(ids.flatMap(id=>[`call:${id}`,`reads:${id}`,`session:${id}`]))
 return Object.fromEntries(Object.entries(expanded).filter(([key])=>valid.has(key)))
}
