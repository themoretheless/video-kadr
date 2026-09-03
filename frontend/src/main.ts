import { mount } from 'svelte'
import { reviewTokenFromPath } from '$lib/reviewRoute.js'
import './app.css'

const target = document.getElementById('app')
if (!target) throw new Error('Missing #app mount target')

const reviewToken = reviewTokenFromPath(location.pathname)
if (reviewToken) {
  const { default: SharedReviewPage } = await import('$lib/components/review/SharedReviewPage.svelte')
  mount(SharedReviewPage, { target, props: { token: reviewToken } })
} else {
  const [{ default: App }, { initStateEffects }] = await Promise.all([
    import('./App.svelte'),
    import('$lib/state/store.svelte.js'),
  ])
  initStateEffects()
  mount(App, { target })

  if (import.meta.env.DEV) {
    void import('$lib/dev/renderGraph/index.js').then(({ mountRenderGraphDevtools }) => {
      mountRenderGraphDevtools()
    })
  }
}
