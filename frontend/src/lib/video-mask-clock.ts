/** Source-relative CFR clock. Decimal inputs are compared as exact rationals. */
export interface VideoMaskFrameRate {readonly numerator:number;readonly denominator:number}
export function isVideoMaskFrameRate(value:unknown):value is VideoMaskFrameRate {
 if(!value||typeof value!=='object'||Array.isArray(value))return false
 const rate=value as VideoMaskFrameRate
 return [rate.numerator,rate.denominator].every(v=>Number.isInteger(v)&&v>0&&v<=4294967295)
}
function clock(rate:VideoMaskFrameRate,frames:number){if(!isVideoMaskFrameRate(rate)||!Number.isSafeInteger(frames)||frames<1||frames>360000)throw new Error('Неверная частота или число кадров видеомаски')}
function position(seconds:string,rate:VideoMaskFrameRate):[bigint,bigint]{
 const text=seconds.trim().replace(',','.')
 if(text.length>64||!/^\d+(?:\.\d{1,18})?$/.test(text))throw new Error('Введите неотрицательное время в секундах')
 const [whole,fraction='']=text.split('.'),scale=10n**BigInt(fraction.length)
 return [(BigInt(whole!)*scale+BigInt(fraction||'0'))*BigInt(rate.numerator),scale*BigInt(rate.denominator)]
}
export function videoMaskFrameAtTime(seconds:string,rate:VideoMaskFrameRate,frames:number):number {
 clock(rate,frames);const[n,d]=position(seconds,rate),frame=n/d
 if(frame>=BigInt(frames))throw new Error('Время за пределами исходного видео')
 return Number(frame)
}
/** Frames intersecting [start,end); an exclusive end on a boundary excludes that frame. */
export function videoMaskFrameRange(start:string,end:string,rate:VideoMaskFrameRate,frames:number):[number,number]{
 clock(rate,frames);const[a,ad]=position(start,rate),[b,bd]=end.trim()?position(end,rate):[BigInt(frames),1n]
 if(a*bd>=b*ad||a>=BigInt(frames)*ad||b>BigInt(frames)*bd)throw new Error('Неверный временной диапазон видеомаски')
 return [Number(a/ad),Number((b+bd-1n)/bd)]
}
/** Round upward so displaying/re-entering an ordinal stays on the same frame. */
export function videoMaskTime(frame:number,rate:VideoMaskFrameRate):string {
 if(!isVideoMaskFrameRate(rate)||!Number.isSafeInteger(frame)||frame<0||frame>360000)throw new Error('Неверный кадр видеомаски')
 const scale=1000000000000n,n=BigInt(frame)*BigInt(rate.denominator)*scale,d=BigInt(rate.numerator),ticks=(n+d-1n)/d
 const fraction=String(ticks%scale).padStart(12,'0').replace(/0+$/,'')
 return String(ticks/scale)+(fraction?'.'+fraction:'')
}

/** Ephemeral brush navigation; decoder ordinals never become floating timestamps. */
export interface VideoMaskNavigation {readonly frame:number;readonly view:'mask'|'source'|'overlay'}
export function isVideoMaskNavigation(value:unknown,frames:number):value is VideoMaskNavigation {
 if(!Number.isSafeInteger(frames)||frames<1||frames>360000||!value||typeof value!=='object'||Array.isArray(value))return false
 const n=value as VideoMaskNavigation
 return Number.isSafeInteger(n.frame)&&n.frame>=0&&n.frame<frames&&['mask','source','overlay'].includes(n.view)&&Object.keys(n).every(k=>k==='frame'||k==='view')
}
