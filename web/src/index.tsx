import { render } from '@solidjs/web'

import { App } from './app'
import { createQueryClient } from './lib/query-client'
import { createAppRouter } from './router'
import './styles.css'

const root = document.getElementById('root')
if (!root) throw new Error('Fixer Web requires a #root element')

const queryClient = createQueryClient()
const router = createAppRouter({ queryClient })

render(() => <App queryClient={queryClient} router={router} />, root)
