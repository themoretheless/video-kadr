import {mount} from 'svelte'
import Scopes from './LiveScopePlayer.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-scope-test')!)
mount(Scopes,{target:document.querySelector('#app')!,props})
