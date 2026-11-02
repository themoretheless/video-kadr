import type {WireConcatPreparation} from './types'
export type ConcatPreparationEdit={kind:'edge';index:number;value:number}|{kind:'flip';axis:'horizontal'|'vertical';value:boolean}|{kind:'reset-crop'}
/** Source Crop::tidy keeps at least one tenth of each image axis. */
export function editConcatPreparation(before:WireConcatPreparation,edit:ConcatPreparationEdit):WireConcatPreparation{
 const crop=[...before.crop] as [number,number,number,number]
 const result={...before,crop,canvas:[...before.canvas] as [number,number]}
 if(edit.kind==='edge'){
  if(!Number.isInteger(edit.index)||edit.index<0||edit.index>3||!Number.isFinite(edit.value))throw new Error('Некорректная граница обрезки Concat')
  crop[edit.index]=Math.max(0,Math.min(0.9,edit.value))
  if(crop[0]+crop[2]>0.9)crop[2]=Math.max(0,0.9-crop[0])
  if(crop[1]+crop[3]>0.9)crop[3]=Math.max(0,0.9-crop[1])
 }else if(edit.kind==='flip'){
  if(typeof edit.value!=='boolean'||!['horizontal','vertical'].includes(edit.axis))throw new Error('Некорректное отражение Concat')
  if(edit.axis==='horizontal')result.flipH=edit.value;else result.flipV=edit.value
 }else if(edit.kind==='reset-crop')result.crop=[0,0,0,0]
 else throw new Error('Неизвестное изменение обрезки')
 return result
}
