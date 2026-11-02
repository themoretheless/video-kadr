import {expect,it} from 'vitest'
import {ttsVoiceLabel} from './tts-voice-label'

it('describes supported Kokoro accents and voices without changing language fallback',()=>{
 expect(ttsVoiceLabel({id:3,name:'af_heart',language:'en'})).toBe('Heart — американский английский · женский')
 expect(ttsVoiceLabel({id:26,name:'bm_george',language:'en'})).toBe('George — британский английский · мужской')
 expect(ttsVoiceLabel({id:45,name:'zf_xiaobei',language:'zh'})).toBe('Xiaobei — китайский · женский')
 expect(ttsVoiceLabel({id:999,name:'unknown',language:'fr'})).toBe('unknown — fr')
})
it('names reference voices by family-independent identity',()=>{
 for(const id of [1000,2000])expect(ttsVoiceLabel({id,name:'internal',language:'en'})).toBe('Bria — английский')
 expect(ttsVoiceLabel({id:1001,name:'pocket_loona',language:'en'})).toBe('Loona — английский')
 for(const id of [1099,2099])expect(ttsVoiceLabel({id,name:'internal',language:'en'})).toBe('Мой образец голоса — английский')
})
