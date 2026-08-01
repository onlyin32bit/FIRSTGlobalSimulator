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
- Set `GAME_SERVER_ORIGIN` to the public `wss://` game server endpoint when match ticketing is enabled.
- Set `N8N_WEBHOOK_URL` only when the public invitation-request form is connected to an automation endpoint. Without it, the API responds honestly with `503` rather than claiming a request was delivered.

## Commands

```sh
pnpm dev
pnpm build
pnpm deploy
pnpm cf-typegen
```

`JWT_SECRET` must match the secret configured for the Rust game server. A missing secret disables ticket issuance and game-server ticket verification.
