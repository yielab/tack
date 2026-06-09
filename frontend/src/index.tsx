/* @refresh reload */
import { render } from 'solid-js/web'
import './index.css'
import App from './app/App.tsx'
import { initTheme } from './shared/state/theme'

initTheme()

const root = document.getElementById('root')

render(() => <App />, root!)
