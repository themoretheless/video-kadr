import type {EasePoints} from './concatRadiusSourceKeys'
export const graphBox={width:280,height:180,left:24,top:16,plotWidth:232,plotHeight:148} as const
export interface GraphRange {scale:number;low:number;high:number}
export function graphRange(points:EasePoints):GraphRange {
 const scale=Math.max(1,Math.abs(points[1]),Math.abs(points[3]))
 return {scale,low:Math.min(0,points[1]/scale,points[3]/scale)-0.25/scale,high:Math.max(1/scale,points[1]/scale,points[3]/scale)+0.25/scale}
}
export function graphPoint(x:number,y:number,range:GraphRange):readonly [number,number] {
 return [graphBox.left+x*graphBox.plotWidth,graphBox.top+(range.high-y/range.scale)/(range.high-range.low)*graphBox.plotHeight]
}
export function clientGraphPoint(x:number,y:number,rect:{left:number;top:number;width:number;height:number},range:GraphRange):readonly [number,number]|null {
 const s=Math.min(rect.width/graphBox.width,rect.height/graphBox.height)
 if(!Number.isFinite(s)||s<=0||!Number.isFinite(x)||!Number.isFinite(y))return null
 const px=(x-rect.left-(rect.width-graphBox.width*s)/2)/s
 const py=(y-rect.top-(rect.height-graphBox.height*s)/2)/s
 const result=[(px-graphBox.left)/graphBox.plotWidth,(range.high-(py-graphBox.top)/graphBox.plotHeight*(range.high-range.low))*range.scale] as const
 return result.every(Number.isFinite)?result:null
}
export function replaceGraphPoint(points:EasePoints,index:0|1,x:number,y:number):EasePoints {
 if(!Number.isFinite(x)||!Number.isFinite(y))throw new Error('Недопустимая точка Безье')
 const next=[...points];next[index*2]=Math.max(0,Math.min(1,x));next[index*2+1]=y
 return next as unknown as EasePoints
}
export function sameGraphPoints(a:EasePoints,b:EasePoints):boolean{return a.every((v,i)=>Math.abs(v-b[i]!)<=1e-12)}
