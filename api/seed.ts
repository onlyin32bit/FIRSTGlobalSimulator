import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { drizzle } from "drizzle-orm/better-sqlite3";
import Database from "better-sqlite3";
import * as schema from "./src/db/schema";
import { v4 as uuidv4 } from 'uuid';

async function seed() {
  const sqlite = new Database(".wrangler/state/v3/d1/miniflare-D1DatabaseObject/2b35d4d42e3c9f6b5ad5b5579a7b1470c66e69f6b33a31e3f5a0095cc6d18656.sqlite");
  const db = drizzle(sqlite, { schema });

  const auth = betterAuth({
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
        }
      }
    }
  });

  try {
    const res = await auth.api.signUpEmail({
      body: {
        email: "admin@fgc.com",
        password: "admin",
        name: "Admin",
        team: "Admin Team"
      }
    });
    console.log("Admin account created!", res);
  } catch (e) {
    console.error("Failed to create admin:", e);
  }
}

seed();
