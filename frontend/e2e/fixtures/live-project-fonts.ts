import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveProjectFontsEditor.svelte'
import {authState} from '../../src/lib/state/auth.svelte'
import {nestedComposition,closeNestedComposition} from '../../src/lib/state/nestedComposition.svelte'
const props=JSON.parse(localStorage.getItem('video-kadr.live-project-fonts-test')!)
closeNestedComposition();authState.token=props.token
nestedComposition.document=props.project;nestedComposition.activeCompositionId='root'
nestedComposition.renderedOutputs={root:'previous-render'}
mount(Editor,{target:document.querySelector('#app')!})
