import {mount} from 'svelte'
import '../../src/app.css'
import Editor from './LiveConcatVideoCutout.svelte'
import {nestedComposition} from '../../src/lib/state/nestedComposition.svelte'
import {getNestedCompositionProject} from '../../src/lib/api'
import type {NestedCompositionProjectDto} from '../../src/lib/api'
const props=JSON.parse(localStorage.getItem('video-kadr.live-concat-video-cutout:'+new URLSearchParams(location.search).get('project'))!) as {token:string;saved:NestedCompositionProjectDto}
props.saved=await getNestedCompositionProject(props.saved.id,props.token)
Object.assign(nestedComposition,{document:props.saved.document,projectId:props.saved.id,revision:props.saved.revision,name:props.saved.name,activeCompositionId:'root',timeUs:0,renderedOutputs:{}})
mount(Editor,{target:document.querySelector('#app')!,props:{token:props.token}})
