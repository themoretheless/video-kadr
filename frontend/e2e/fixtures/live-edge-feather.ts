import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveEdgeFeatherEditor.svelte'
import {nestedComposition} from '../../src/lib/state/nestedComposition.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-edge-feather-test')!)
nestedComposition.document=props.project
nestedComposition.activeCompositionId='root'
nestedComposition.timeUs=500000
mount(Editor,{target:document.querySelector('#app')!,props:{token:props.token}})
