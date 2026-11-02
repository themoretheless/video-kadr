import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveLightningEditor.svelte'
import {nestedComposition} from '../../src/lib/state/nestedComposition.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-lightning-test')!)
nestedComposition.document=props.project
nestedComposition.activeCompositionId='root'
nestedComposition.timeUs=props.timeUs
mount(Editor,{target:document.querySelector('#app')!,props:{token:props.token}})
