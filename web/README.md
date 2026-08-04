# FGC Simulator web app

The SvelteKit application is the public browser origin. All browser API calls use same-origin `/api/...` URLs:

- In local development, Vite proxies `/api` to `http://localhost:8787`.
- In production, `web/src/routes/api/[...path]/+server.ts` forwards requests to the API Worker through the `API` Cloudflare service binding.

This keeps Better Auth session cookies first-party and avoids a browser CORS dependency.

## Local development

1. Start the API worker from `api/` on port 8787.
2. Start the web app:

   ```sh
   pnpm dev
   ```

3. Open the web app on `http://localhost:5173`.

Use an invitation code issued by an administrator to create an account. Authenticated game pages redirect guests to `/auth` and preserve the requested path in `next`.

## Production

Deploy the API Worker, then deploy this worker with its `API` service binding configured in `wrangler.jsonc`. Regenerate configuration types whenever bindings change:

```sh
pnpm gen
pnpm check
pnpm build
```

Do not configure a public API URL for normal deployments—the browser should continue using its same-origin `/api` route.
