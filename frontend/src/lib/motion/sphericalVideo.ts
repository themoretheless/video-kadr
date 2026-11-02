import type {WireFisheyeLens} from '$lib/composition/types'
type Quaternion = readonly [number,number,number,number]
export interface Orientation { readonly timeSeconds:number;readonly quaternion:Quaternion }
export const defaultLenses:readonly [WireFisheyeLens,WireFisheyeLens]=[
  {center:[.25,.5],radius:[.25,.5],fov:190,rotation:[0,0,0]},
  {center:[.75,.5],radius:[.25,.5],fov:190,rotation:[0,180,0]},
]
function record(v:unknown):v is Record<string,unknown>{return typeof v==='object'&&v!==null&&!Array.isArray(v)}
function vector(v:unknown,n:number):v is number[]{return Array.isArray(v)&&v.length===n&&v.every(x=>typeof x==='number'&&Number.isFinite(x))}
function fail(message:string):never{throw new Error(message)}
export function parseLensCalibration(value:unknown):readonly [WireFisheyeLens,WireFisheyeLens]{
  if(!Array.isArray(value)||value.length!==2)fail('Нужны два объектива')
  for(const lens of value){if(!record(lens)||Object.keys(lens).some(k=>!['center','radius','fov','rotation'].includes(k))||!vector(lens.center,2)||lens.center.some(v=>v<0||v>1)||!vector(lens.radius,2)||lens.radius.some(v=>v<.001||v>1)||!vector(lens.rotation,3)||typeof lens.fov!=='number'||!Number.isFinite(lens.fov)||lens.fov<180||lens.fov>240)fail('Некорректная калибровка equidistant fisheye')}
  return value as [WireFisheyeLens,WireFisheyeLens]
}
function multiply(a:Quaternion,b:Quaternion):Quaternion{return [a[0]*b[0]-a[1]*b[1]-a[2]*b[2]-a[3]*b[3],a[0]*b[1]+a[1]*b[0]+a[2]*b[3]-a[3]*b[2],a[0]*b[2]-a[1]*b[3]+a[2]*b[0]+a[3]*b[1],a[0]*b[3]+a[1]*b[2]-a[2]*b[1]+a[3]*b[0]]}
/** Explicit sidecar, never guesses the proprietary INSV trailer, units or axis mapping. */
export function parseOrientationSidecar(value:unknown):readonly Orientation[]{
  if(!record(value)||value.version!==1||Object.keys(value).some(k=>!['version','samples','gyro'].includes(k))||('samples' in value)===('gyro' in value))fail('Ожидается {version:1,samples:[…]} или {version:1,gyro:[…]}')
  const gyro='gyro' in value;const rows=gyro?value.gyro:value.samples
  if(!Array.isArray(rows)||!rows.length||rows.length>20000)fail('Нужно от 1 до 20000 отсчётов')
  let previous=-1;let lastRate=[0,0,0];let q:Quaternion=[1,0,0,0];const output:Orientation[]=[]
  for(const row of rows){
    if(!record(row)||typeof row.timeSeconds!=='number'||!Number.isFinite(row.timeSeconds)||row.timeSeconds<0||row.timeSeconds<=previous||Object.keys(row).some(k=>!['timeSeconds',gyro?'radiansPerSecond':'quaternion'].includes(k)))fail('Время отсчётов должно строго возрастать')
    const time=row.timeSeconds
    if(gyro){
      if(!vector(row.radiansPerSecond,3)||row.radiansPerSecond.some(v=>Math.abs(v)>1000))fail('gyro: нужны три компоненты в рад/с')
      const rate=row.radiansPerSecond
      if(previous>=0){const dt=time-previous;const v=rate.map((v,i)=>(v+lastRate[i])*.5*dt);const angle=Math.hypot(...v);if(!Number.isFinite(angle))fail('Слишком большой интервал gyro');if(angle>0){const s=Math.sin(angle*.5)/angle;q=multiply(q,[Math.cos(angle*.5),v[0]*s,v[1]*s,v[2]*s]);const n=Math.hypot(...q);q=q.map(v=>v/n) as unknown as Quaternion}}
      lastRate=rate
    }else{
      if(!vector(row.quaternion,4))fail('Нужен кватернион [w,x,y,z]')
      const n=row.quaternion.reduce((s,v)=>s+v*v,0);if(n<.99||n>1.01)fail('Кватернион должен иметь единичную длину')
      q=row.quaternion.map(v=>v/Math.sqrt(n)) as unknown as Quaternion
    }
    output.push({timeSeconds:time,quaternion:q});previous=time
  }
  return output
}
