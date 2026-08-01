import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { logger } from 'hono/logger'
import { jsonError } from './responses'
import type { Bindings } from './types'

import authRoutes from './routes/auth'
import userRoutes from './routes/user'
import robotRoutes from './routes/robot'
import matchesRoutes from './routes/matches'
import gamePacksRoutes from './routes/game-packs'
import adminRoutes from './routes/admin'

const app = new Hono<{ Bindings: Bindings }>()

app.use('*', logger())

app.use('*', cors({
  origin: (origin, c) => {
    const webOrigin = (c.env.WEB_ORIGIN || 'http://localhost:5173').replace(/\/$/, '')
    const apiOrigin = (c.env.API_ORIGIN || new URL(c.req.url).origin).replace(/\/$/, '')
    return origin === webOrigin || origin === apiOrigin ? origin : null
  },
  credentials: true,
  allowHeaders: ['Content-Type', 'Authorization'],
  allowMethods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS']
}))

app.route('/api/auth', authRoutes)
app.route('/api/user', userRoutes)
app.route('/api/robot', robotRoutes)
app.route('/api/matches', matchesRoutes)
app.route('/api/game-packs', gamePacksRoutes)
app.route('/api/admin', adminRoutes)

app.get('/', (c) => {
  return c.text('FGC 2026 Simulator API (Hono)')
})

app.onError((err, c) => {
  console.error('Unhandled Exception:', err)
  return jsonError(c, 500, 'INTERNAL_ERROR', err.message || 'An unexpected error occurred')
})

export default app
