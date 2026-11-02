import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveTrackMatteEditor.svelte'
import {nestedComposition} from '../../src/lib/state/nestedComposition.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-track-matte-test')!)
nestedComposition.document=props.project
nestedComposition.activeCompositionId='root'
nestedComposition.timeUs=1500000
mount(Editor,{target:document.querySelector('#app')!,props:{token:props.token}})
