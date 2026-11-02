import type { WireCanvas } from './types'
/** Comparison controls preserve an off-grid reference while stepping by one project frame. */
export function stepComparisonReference(referenceUs:number,direction:-1|1,canvas:Pick<WireCanvas,'fpsMilli'|'fpsRate'>):number {
 const numerator=canvas.fpsRate?.numerator ?? canvas.fpsMilli
 const denominator=canvas.fpsRate?.denominator ?? 1000
 if(!Number.isSafeInteger(referenceUs)||referenceUs<0||!Number.isSafeInteger(numerator)||numerator<1||!Number.isSafeInteger(denominator)||denominator<1)throw new Error('Некорректное время сравнения')
 const step=Math.round(1000000*denominator/numerator)
 const next=Math.max(0,referenceUs+direction*step)
 if(step<1||!Number.isSafeInteger(next))throw new Error('Время сравнения вне диапазона')
 return next
}
