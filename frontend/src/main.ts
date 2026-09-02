import { mount } from 'svelte'
import App from './App.svelte'
import { initStateEffects } from '$lib/state/store.svelte.js'
import './app.css'

initStateEffects()

const target = document.getElementById('app')
if (!target) throw new Error('Missing #app mount target')

mount(App, { target })

if (import.meta.env.DEV) {
  void import('$lib/dev/renderGraph/index.js').then(({ mountRenderGraphDevtools }) => {
    mountRenderGraphDevtools()
  })
}
