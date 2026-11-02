import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveBasic3DEditor.svelte'
import {nestedComposition} from '../../src/lib/state/nestedComposition.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-basic-3d-test')!)
nestedComposition.document=props.project
nestedComposition.activeCompositionId='root'
nestedComposition.timeUs=500000
mount(Editor,{target:document.querySelector('#app')!,props:{token:props.token}})
