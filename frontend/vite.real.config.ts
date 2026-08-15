import { mergeConfig } from 'vite'
import baseConfig from './vite.config.js'

const backendUrl = process.env.REAL_COMPOSITION_BACKEND_URL
if (!backendUrl) throw new Error('REAL_COMPOSITION_BACKEND_URL is required')

export default mergeConfig(baseConfig, {
  server: {
    proxy: {
      '/api': backendUrl,
      '/files': backendUrl,
    },
  },
})
