import type {NativeNestedProject} from '../api'
import {MAX_TEXT_LENGTH} from './types'

/** Literal cue text edit: no clock, style, transform or retained animation changes. */
export function setNativeCaptionText(project:NativeNestedProject,compositionId:string,clipId:string,text:string,expectedText:string):NativeNestedProject {
  if(typeof text!=='string'||!text.trim()||text.includes('\0')||Array.from(text).length>MAX_TEXT_LENGTH)throw new Error(`Текст должен содержать от 1 до ${MAX_TEXT_LENGTH} символов без NUL`)
  const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
  if(!node)throw new Error('Композиция не найдена')
  const track=node.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId))
  if(!track||track.kind!=='text'||!node.content.metadata?.captionTextTracks?.includes(track.id))throw new Error('Слой не является субтитром')
  if(track.locked)throw new Error('Дорожка субтитров заблокирована')
  const clip=track.clips.find(clip=>clip.id===clipId)!
  if(clip.text!==expectedText)throw new Error('Текст реплики изменился; сбросьте черновик и повторите')
  if(text===clip.text)return project
  return {...project,compositions:{...project.compositions,[compositionId]:{...node,content:{...node.content,tracks:node.content.tracks.map(candidate=>candidate.id===track.id?{...track,clips:track.clips.map(candidate=>candidate.id===clipId?{...candidate,text}:candidate)}:candidate)}}}}
}
