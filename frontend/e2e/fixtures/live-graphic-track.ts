import { mount } from 'svelte'
import '../../src/app.css'
import Editor from './LiveGraphicTrackEditor.svelte'
import { authState } from '../../src/lib/state/auth.svelte'
import { compositionState, resetCompositionForTests } from '../../src/lib/state/composition.svelte'
import { normalizeComposition } from '../../src/lib/composition/validation'
const props = JSON.parse(localStorage.getItem('video-kadr.live-graphic-track-test')!)
resetCompositionForTests()
authState.token = props.token
compositionState.document = normalizeComposition(props.document)
compositionState.transport.playheadTicks = 1500000
compositionState.ui.selectedTrackId = 'graphics'
compositionState.ui.selectedClipId = 'graphic'
mount(Editor, { target: document.querySelector('#app')! })
