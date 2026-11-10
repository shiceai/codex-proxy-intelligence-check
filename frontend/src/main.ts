// @env browser
import { createApp } from 'vue'
import App from './App.vue'
import { installPreviewHost } from './preview'
import '@codex-proxy/ui/styles.css'
import './styles/index.css'

const preview = installPreviewHost()
createApp(App, { preview }).mount('#app')
