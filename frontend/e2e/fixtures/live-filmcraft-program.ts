import { mount } from 'svelte'
import type { MediaEntry } from '../../src/lib/types'
import Panel from '../../src/lib/components/composition/FilmcraftProjectPanel.svelte'
const props = JSON.parse(localStorage.getItem('video-kadr.filmcraft-program-test') ?? '{"token":null,"library":[]}') as { token: string | null; library: MediaEntry[] }
mount(Panel, { target: document.querySelector('#app')!, props })
