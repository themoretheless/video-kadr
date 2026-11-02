import type {WireAnimatableValue} from '$lib/composition/types'
import type {GrayFrame} from './tracker'

export const MAX_SPHERICAL_TRACK_FRAMES=300
const RADIUS=3, MIN_SCORE=.78, RECOVERY_SCORE=.9, MAX_GAP_SECONDS=2
const MAX_COMPARISONS=80_000_000
interface Point {x:number;y:number;scale:number}
interface Template {values:number[];energy:number}
interface Match extends Point {score:number;anchorScore:number}
export interface SphericalTrackingResult {
  pan:WireAnimatableValue
  tilt:WireAnimatableValue
  /** No measurements are emitted during these intervals; camera keys interpolate their endpoints. */
  recoveredGaps:readonly {fromSeconds:number;toSeconds:number}[]
}

/** Selected-texture tracking; immutable identity template, motion prediction and bounded reacquisition. */
export function trackSphericalSubject(frames:readonly GrayFrame[],times:readonly number[],initial:readonly [number,number]):SphericalTrackingResult {
  if(frames.length<2||frames.length>MAX_SPHERICAL_TRACK_FRAMES||times.length!==frames.length||times.some((t,i)=>!Number.isFinite(t)||t<0||!Number.isSafeInteger(Math.round(t*1e6))||(i>0&&Math.round(t*1e6)<=Math.round(times[i-1]!*1e6))))throw new Error(`Нужно 2..${MAX_SPHERICAL_TRACK_FRAMES} кадров со строго возрастающим временем`)
  if(initial.length!==2||initial.some(v=>!Number.isFinite(v)||v<0||v>1))throw new Error('Точка должна находиться внутри панорамы')
  const {width,height}=frames[0]!
  if(!Number.isSafeInteger(width)||!Number.isSafeInteger(height)||width<32||height<16||frames.some(f=>f.width!==width||f.height!==height||!(f.data instanceof Uint8Array)||f.data.length!==width*height)||width*height*frames.length>16*1024*1024)throw new Error('Превышен размер кадров tracking')
  const start:Point={x:Math.min(width-1,Math.floor(initial[0]*width)),y:Math.min(height-1,Math.floor(initial[1]*height)),scale:1}
  if(start.y<RADIUS+1||start.y>=height-RADIUS-1)throw new Error('Выберите точку дальше от полюсов панорамы')
  const anchor=template(frames[0]!,start)
  if(anchor.energy<100)throw new Error('Выберите контрастный фрагмент объекта')
  let adaptive=anchor, comparisons=0, last=start, lastIndex=0, velocity={x:0,y:0},gapStart:number|undefined
  const measured=[{...start,index:0}],recoveredGaps:{fromSeconds:number;toSeconds:number}[]=[]
  const wrap=(x:number)=>((x%width)+width)%width
  const longitudeDelta=(a:number,b:number)=>((a-b+width*1.5)%width)-width/2
  function score(frame:GrayFrame,p:Point):Match|null {
    comparisons+=(RADIUS*2+1)**2
    if(comparisons>MAX_COMPARISONS)throw new Error('Превышен лимит вычислений tracking; сократите интервал')
    if(p.y-RADIUS*p.scale<0||p.y+RADIUS*p.scale>=height)return null
    const candidate=template(frame,p)
    if(candidate.energy<100)return null
    const correlation=(reference:Template)=>candidate.values.reduce((dot,v,i)=>dot+v*reference.values[i]!,0)/Math.sqrt(reference.energy*candidate.energy)
    const identity=correlation(anchor),recent=correlation(adaptive)
    if(identity<.55)return null
    return {...p,score:identity*.7+recent*.3,anchorScore:identity}
  }
  function find(frame:GrayFrame,origin:Point,global:boolean):Match|null {
    const matches:Match[]=[]
    const consider=(x:number,y:number,scale:number)=>{const candidate=score(frame,{x:wrap(x),y,scale});if(candidate)matches.push(candidate)}
    // A fixed anchor prevents adapting the template onto the occluder. Nearby scales allow gradual size changes.
    const scales=[Math.max(.6,origin.scale*.85),origin.scale,Math.min(1.8,origin.scale*1.18)]
    if(global){for(let y=4;y<height-4;y+=2)for(let x=0;x<width;x+=2)for(const scale of scales)consider(x,y,scale)}
    else for(let y=Math.max(4,Math.round(origin.y)-8);y<=Math.min(height-5,Math.round(origin.y)+8);y++)for(let dx=-8;dx<=8;dx++)for(const scale of scales)consider(Math.round(origin.x)+dx,y,scale)
    matches.sort((a,b)=>b.score-a.score||Math.abs(longitudeDelta(a.x,origin.x))-Math.abs(longitudeDelta(b.x,origin.x)))
    if(global&&matches[0]){const coarse=matches[0];for(let dy=-1;dy<=1;dy++)for(let dx=-1;dx<=1;dx++)for(const scale of scales)consider(coarse.x+dx,coarse.y+dy,scale);matches.sort((a,b)=>b.score-a.score)}
    const best=matches[0];if(!best)return null
    const competitor=matches.find(p=>longitudeDelta(p.x,best.x)**2+(p.y-best.y)**2>16)
    // A matching second object is ambiguous: keep the gap open, never silently switch identity.
    if(competitor&&best.score-competitor.score<(global?.08:.04))return null
    if(best.score<(global?RECOVERY_SCORE:MIN_SCORE))return null
    return best
  }
  for(let i=1;i<frames.length;i++){
    const elapsed=times[i]!-times[lastIndex]!
    if(gapStart!==undefined&&elapsed>MAX_GAP_SECONDS)throw new Error(`Точка потеряна на кадре ${gapStart}; ключи не изменены`)
    const predicted={x:wrap(last.x+velocity.x*elapsed),y:last.y+velocity.y*elapsed,scale:last.scale}
    let match=gapStart===undefined?find(frames[i]!,predicted,false):null
    if(!match)match=find(frames[i]!,last,true)
    if(!match){gapStart??=i;continue}
    if(gapStart!==undefined){recoveredGaps.push({fromSeconds:times[lastIndex]!,toSeconds:times[i]!});gapStart=undefined}
    velocity={x:longitudeDelta(match.x,last.x)/elapsed,y:(match.y-last.y)/elapsed}
    last=match;lastIndex=i;measured.push({...match,index:i})
    if(match.score>.9&&match.anchorScore>.85)adaptive=template(frames[i]!,match)
  }
  if(gapStart!==undefined)throw new Error(`Точка потеряна на кадре ${gapStart}; ключи не изменены`)
  const pan=measured.map(p=>((p.x+.5)/width-.5)*360)
  for(let i=1;i<pan.length;i++){while(pan[i]!-pan[i-1]!>180)pan[i]!-=360;while(pan[i]!-pan[i-1]!< -180)pan[i]!+=360}
  const channel=(values:readonly number[]):WireAnimatableValue=>({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:values.map((value,i)=>({tick:Math.round(times[measured[i]!.index]!*1e6),value}))}})
  return {pan:channel(pan),tilt:channel(measured.map(p=>((p.y+.5)/height-.5)*180)),recoveredGaps}
}

/** Wrap longitude in the source raster, avoiding the previous threefold frame allocation. */
function template(frame:GrayFrame,point:Point):Template {
  const values:number[]=[]
  for(let y=-RADIUS;y<=RADIUS;y++)for(let x=-RADIUS;x<=RADIUS;x++){
    const sx=((Math.round(point.x+x*point.scale)%frame.width)+frame.width)%frame.width
    const sy=Math.round(point.y+y*point.scale)
    values.push(frame.data[sy*frame.width+sx]??0)
  }
  const mean=values.reduce((a,b)=>a+b,0)/values.length
  let energy=0;for(let i=0;i<values.length;i++){values[i]!-=mean;energy+=values[i]!**2}
  return {values,energy}
}
