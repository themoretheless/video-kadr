import {mount} from 'svelte'
import Graph from '../../src/lib/components/composition/NativeScopeGraph.svelte'
import type {NativeScopeGraph} from '../../src/lib/api'
const graph=await fetch('/e2e/fixtures/native-parade.json').then(response=>response.json()) as NativeScopeGraph
mount(Graph,{target:document.querySelector('#app')!,props:{graph,height:256}})
