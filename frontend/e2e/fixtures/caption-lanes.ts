import { mount } from 'svelte'
import '../../src/app.css'
import Editor from './CaptionLanesEditor.svelte'
import { authState } from '../../src/lib/state/auth.svelte'
import { newComposition } from '../../src/lib/state/composition.svelte'
authState.token = localStorage.getItem('caption-lanes-token')
newComposition()
mount(Editor, { target: document.querySelector('#app')! })
