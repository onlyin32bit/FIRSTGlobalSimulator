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
import gameServerRoutes from './routes/game-servers'

export { MatchLobby } from './match-lobby'

const app = new Hono<{ Bindings: Bindings }>()

app.use('*', logger())

app.use('*', cors({
  origin: (origin, c) => {
    if (!origin) return '*'
    const webOrigin = (c.env.WEB_ORIGIN || 'http://localhost:5173').replace(/\/$/, '')
    const apiOrigin = (c.env.API_ORIGIN || new URL(c.req.url).origin).replace(/\/$/, '')
    const norm = origin.replace(/\/$/, '')
    if (norm === webOrigin || norm === apiOrigin) return origin
    if (norm.startsWith('http://localhost:') || norm.startsWith('http://127.0.0.1:')) return origin
    return null
  },
  credentials: true,
  allowHeaders: ['Content-Type', 'Authorization', 'Cookie', 'X-Requested-With', 'Accept', 'Origin'],
  allowMethods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
  exposeHeaders: ['Set-Cookie']
}))

app.route('/api/auth', authRoutes)
app.route('/api/user', userRoutes)
app.route('/api/robot', robotRoutes)
app.route('/api/matches', matchesRoutes)
app.route('/api/game-packs', gamePacksRoutes)
app.route('/api/admin', adminRoutes)
app.route('/api/game-servers', gameServerRoutes)

app.get('/', (c) => {
  return c.text('FGC 2026 Simulator API (Hono)')
})

app.onError((err, c) => {
  console.error('Unhandled Exception:', err)
  return jsonError(c, 500, 'INTERNAL_ERROR', err.message || 'An unexpected error occurred')
})

export default app
