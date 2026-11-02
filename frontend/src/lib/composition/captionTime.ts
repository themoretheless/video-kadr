/** Numbers are legacy microseconds; strings are exact signed Filmcraft ticks. */
export type CaptionTimestamp=number|string
export const CAPTION_SOURCE_TICKS_PER_MICROSECOND=254016n
export const CAPTION_SOURCE_TICKS_PER_SECOND=254016000000n
const MIN_SOURCE_TICK=-(1n<<63n),MAX_SOURCE_TICK=(1n<<63n)-1n
export function exactCaptionTicks(time:CaptionTimestamp):bigint {
 if(typeof time==='number') {
  if(!Number.isSafeInteger(time)||time<0)throw new Error('Некорректное время субтитра')
  return BigInt(time)*CAPTION_SOURCE_TICKS_PER_MICROSECOND
 }
 if(!/^[+-]?\d+$/.test(time))throw new Error('Некорректные тики субтитра')
 const ticks=BigInt(time)
 if(ticks<MIN_SOURCE_TICK||ticks>MAX_SOURCE_TICK)throw new Error('Время субтитра вне диапазона')
 return ticks
}
export function compareCaptionTimes(a:CaptionTimestamp,b:CaptionTimestamp):-1|0|1 {
 const left=exactCaptionTicks(a),right=exactCaptionTicks(b)
 return left<right?-1:left>right?1:0
}
/** Approximate presentation only. Keep the original timestamp in authored data. */
export function captionDisplaySeconds(time:CaptionTimestamp):number {
 return Number(exactCaptionTicks(time))/Number(CAPTION_SOURCE_TICKS_PER_SECOND)
}

/** Signed edit distances use the same wire units, with negative numeric us allowed. */
export function exactCaptionDeltaTicks(delta:CaptionTimestamp):bigint {
 if(typeof delta==='number') {
  if(!Number.isSafeInteger(delta))throw new Error('Некорректный сдвиг субтитра')
  return BigInt(delta)*CAPTION_SOURCE_TICKS_PER_MICROSECOND
 }
 return exactCaptionTicks(delta)
}

export function captionMicrosecondsFloor(time:CaptionTimestamp):number {
 const ticks=exactCaptionTicks(time)
 if(ticks<0n)throw new Error('Отрицательное время субтитра')
 return Number(ticks/CAPTION_SOURCE_TICKS_PER_MICROSECOND)
}
export function captionMicrosecondsCeil(time:CaptionTimestamp):number {
 const ticks=exactCaptionTicks(time)
 if(ticks<0n)throw new Error('Отрицательное время субтитра')
 return Number((ticks+CAPTION_SOURCE_TICKS_PER_MICROSECOND-1n)/CAPTION_SOURCE_TICKS_PER_MICROSECOND)
}
export function captionTimestampFromExact(ticks:bigint,original:CaptionTimestamp):CaptionTimestamp {
 if(ticks<0n)throw new Error('Отрицательное время субтитра')
 if(typeof original==='number'&&ticks%CAPTION_SOURCE_TICKS_PER_MICROSECOND===0n){
  const value=Number(ticks/CAPTION_SOURCE_TICKS_PER_MICROSECOND)
  if(!Number.isSafeInteger(value))throw new Error('Время субтитра вне диапазона')
  return value
 }
 if(ticks>MAX_SOURCE_TICK)throw new Error('Время субтитра вне диапазона')
 return ticks.toString()
}
export function extractCaptionSpan(start:CaptionTimestamp,end:CaptionTimestamp,a:CaptionTimestamp,b:CaptionTimestamp):readonly [CaptionTimestamp,CaptionTimestamp]|null {
 const [s,e,left,right]=[start,end,a,b].map(exactCaptionTicks)
 if(s<0n||left<0n||e<=s||right<=left)throw new Error('Некорректный диапазон субтитров')
 if(e<=left)return [start,end]
 if(s>=right)return [captionTimestampFromExact(s-(right-left),start),captionTimestampFromExact(e-(right-left),end)]
 const kept=(left>s?left-s:0n)+(e>right?e-right:0n)
 if(kept===0n)return null
 const nextStart=s<left?s:left
 return [captionTimestampFromExact(nextStart,start),captionTimestampFromExact(nextStart+kept,end)]
}

/** Exact source tick_of(frame), including Euclidean floor for negative/exotic rates. */
export function captionFrameDelta(frames:number,rate:{readonly numerator:number;readonly denominator:number}):string {
 if(!Number.isSafeInteger(frames)||!Number.isSafeInteger(rate.numerator)||rate.numerator<=0||!Number.isSafeInteger(rate.denominator)||rate.denominator<=0)throw new Error('Некорректное число кадров или частота')
 const numerator=BigInt(frames)*CAPTION_SOURCE_TICKS_PER_SECOND*BigInt(rate.denominator),denominator=BigInt(rate.numerator)
 const ticks=numerator/denominator-(numerator<0n&&numerator%denominator!==0n?1n:0n)
 if(ticks<MIN_SOURCE_TICK||ticks>MAX_SOURCE_TICK)throw new Error('Сдвиг субтитра вне диапазона')
 return ticks.toString()
}
