import { Hono } from 'hono'
import { jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

app.get('/', (c) => {
  return jsonSuccess(c, {
    packs: [
      { id: 'fgc-2026', name: 'Igniting Innovation', version: '1.0.0' }
    ]
  })
})

export default app
