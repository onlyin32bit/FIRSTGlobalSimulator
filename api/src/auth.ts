import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "./db/schema";

export type AuthEnvironment = {
  DB: D1Database
  /** Public origin of this API, such as https://api.example.com. */
  API_ORIGIN?: string
  /** Public web-app origin allowed to use the API. */
  WEB_ORIGIN?: string
}

export function createAuth(env: AuthEnvironment, requestUrl?: string) {
  const db = drizzle(env.DB, { schema });
  
  // Do not infer a public URL from a proxied request in production. The host seen
  // by the Worker may be an internal origin, which causes Better Auth to reject
  // otherwise-valid browser requests and issue cookies for the wrong host.
  const requestOrigin = requestUrl ? new URL(requestUrl).origin : 'http://localhost:8787'
  const apiOrigin = (env.API_ORIGIN || requestOrigin).replace(/\/$/, '')
  const webOrigin = (env.WEB_ORIGIN || 'http://localhost:5173').replace(/\/$/, '')
  const baseURL = `${apiOrigin}/api/auth`

  const trustedOrigins = Array.from(new Set([
    webOrigin,
    apiOrigin,
    'http://localhost:5173',
    'http://127.0.0.1:5173',
    'http://localhost:4173',
    'http://127.0.0.1:4173',
    'http://localhost:8787'
  ]))

  return betterAuth({
    baseURL,
    trustedOrigins,
    database: drizzleAdapter(db, {
      provider: "sqlite",
    }),
    emailAndPassword: {
      enabled: true,
    },
    user: {
      additionalFields: {
        team: {
          type: "string",
          required: true
        },
        role: {
          type: "string",
          required: false,
          defaultValue: "user",
          input: false
        }
      }
    }
  });
}
