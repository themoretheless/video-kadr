import type {Insta360DriftCorrection} from '$lib/api'
const record=(v:unknown):v is Record<string,unknown>=>typeof v==='object'&&v!==null&&!Array.isArray(v)
const finite=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)
/** Advanced observations remain explicit; a slow pan is never labelled stationary automatically. */
export function parseDriftCorrection(value:unknown,duration:number):Insta360DriftCorrection{
 if(!record(value)||Object.keys(value).some(k=>!['biasRadiansPerSecond','stationaryIntervals','headingAnchors'].includes(k)))throw new Error('Неизвестные поля калибровки дрейфа')
 const bias=value.biasRadiansPerSecond
 if(bias!==undefined&&(!Array.isArray(bias)||bias.length!==3||!bias.every(finite)||Math.hypot(...bias)>.2))throw new Error('Смещение gyro: три компоненты в рад/с, длина до 0.2')
 const windows=value.stationaryIntervals??[],anchors=value.headingAnchors??[]
 if(!Array.isArray(windows)||windows.length>16||!Array.isArray(anchors)||anchors.length>256)throw new Error('Слишком много наблюдений калибровки')
 let previous=-1,total=0
 for(const w of windows){if(!record(w)||Object.keys(w).some(k=>!['startSeconds','endSeconds'].includes(k))||!finite(w.startSeconds)||!finite(w.endSeconds)||w.startSeconds<0||w.startSeconds<previous||w.endSeconds>duration||w.endSeconds-w.startSeconds<.5)throw new Error('Неподвижные интервалы должны возрастать и длиться хотя бы 0.5 с');previous=w.endSeconds;total+=w.endSeconds-w.startSeconds}
 if(total>60||bias!==undefined&&windows.length)throw new Error('Укажите смещение или неподвижные интервалы, суммарно до 60 с')
 previous=-1
 if(anchors.length===1)throw new Error('Нужны хотя бы два измерения направления')
 for(const a of anchors){if(!record(a)||Object.keys(a).some(k=>!['timeSeconds','headingDegrees'].includes(k))||!finite(a.timeSeconds)||!finite(a.headingDegrees)||a.timeSeconds<0||a.timeSeconds<=previous||a.timeSeconds>duration)throw new Error('Измерения направления должны возрастать внутри клипа');previous=a.timeSeconds}
 return value as Insta360DriftCorrection
}
