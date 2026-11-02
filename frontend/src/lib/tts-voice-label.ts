import type {LocalTtsVoice} from './api'

export function ttsVoiceLabel(voice:LocalTtsVoice):string {
 const language=voice.language==='en'?'английский':voice.language==='zh'?'китайский':voice.language
 if(voice.id===1099||voice.id===2099)return `Мой образец голоса — ${language}`
 if(voice.id===1000||voice.id===2000)return `Bria — ${language}`
 if(voice.id===1001)return `Loona — ${language}`
 const match=/^([abz])([fm])_(.+)$/.exec(voice.name)
 if(!match)return `${voice.name} — ${language}`
 const [,accent,gender,name]=match
 const title=name.charAt(0).toUpperCase()+name.slice(1)
 const detail=accent==='a'?'американский английский':accent==='b'?'британский английский':'китайский'
 return `${title} — ${detail} · ${gender==='f'?'женский':'мужской'}`
}
