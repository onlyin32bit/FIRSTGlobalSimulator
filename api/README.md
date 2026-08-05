# FGC Simulator API

The API is a Cloudflare Worker backed by D1. The web Worker calls it through a service binding, while local web development proxies `/api` to this worker on port `8787`.

## Local setup

1. Install dependencies: `pnpm install` in `api/` and `web/`.
2. Create `api/.dev.vars` with a local ticket secret:

   ```dotenv
   JWT_SECRET=replace-with-a-long-random-local-secret
   N8N_WEBHOOK_URL=http://localhost:5678/webhook/invite-request
   ```

3. Apply D1 migrations locally:

   ```sh
   pnpm wrangler d1 migrations apply fgc26-local --local
   ```

4. Start the API: `pnpm dev`.
5. Start the web app separately: `pnpm --dir ../web dev`.

The first administrator must be promoted through D1 during bootstrap. Thereafter, administrators can create, revoke, list, and copy invitation codes through the protected `/api/admin/invitations` REST endpoints.

## Production

- Deploy the API worker first.
- Configure the web worker service binding `API` to point at this worker (already declared in `web/wrangler.jsonc`).
- Set `WEB_ORIGIN` to the public web origin and `API_ORIGIN` to the API worker origin.
- Store `JWT_SECRET` with `wrangler secret put JWT_SECRET`; do not set it in `wrangler.jsonc` or commit it.
- The Worker packages `../pkgs/games` as its `PACK_ASSETS` binding. It is the only service that serves pack assets and authoritative pack source; do not mount `pkgs/games` on game hosts.
- In the admin control center, create a Game server and copy its one-time key to the host as `GAME_SERVER_KEY`.
- Configure the Rust host with `API_URL=https://your-api.example.com`, `GAME_SERVER_KEY=...`, and optional `GAME_SERVER_MAX_USERS=50`, `GAME_SERVER_MAX_MATCHES=10`, and `GAME_SERVER_SLOTS=10`. At startup it fetches `/api/game-packs/:id/runtime`, compiles the received physics, semantics, and Rhai sources in memory, then heartbeats every 10 seconds. Ticket routing only uses hosts whose heartbeat is less than 30 seconds old and whose reported capacity is not full.
- The API alone signs and validates match tickets. A host exchanges the browser ticket for claims at `/api/game-servers/tickets/verify` using its generated server key, so `JWT_SECRET` is never deployed to a game server.
- Browsers use `/metadata` and `/assets` only. `/metadata` contains the visual/debug subset and never includes raw Rhai source; the GLB remains an API asset and is not shipped through the game server.
- Set `N8N_WEBHOOK_URL` only when the public invitation-request form is connected to an automation endpoint. Without it, the API responds honestly with `503` rather than claiming a request was delivered.

## Commands

```sh
pnpm dev
pnpm build
pnpm deploy
pnpm cf-typegen
```

`JWT_SECRET` is an API-only secret. A missing secret disables ticket issuance and API ticket verification.
