import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { drizzle } from "drizzle-orm/better-sqlite3";
import Database from "better-sqlite3";
import * as schema from "./src/db/schema";
import * as path from "path";
import * as fs from "fs";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function findDbFile(): string {
  const dir = path.join(
    __dirname,
    ".wrangler", "state", "v3", "d1", "miniflare-D1DatabaseObject"
  );

  if (!fs.existsSync(dir)) {
    throw new Error(
      `D1 state directory not found: ${dir}\n` +
      `Run "pnpm dev" once first so Wrangler creates the local database.`
    );
  }

  const files = fs.readdirSync(dir)
    .filter(f => f.endsWith(".sqlite") && f !== "metadata.sqlite")
    .map(f => path.join(dir, f));

  if (files.length === 0) {
    throw new Error(
      `No D1 SQLite file found in: ${dir}\n` +
      `Run "pnpm dev" once first so Wrangler creates the local database.`
    );
  }

  return files[0];
}

function applyMigrations(sqlite: InstanceType<typeof Database>) {
  const migrationsDir = path.join(__dirname, "drizzle");
  const files = fs.readdirSync(migrationsDir)
    .filter(f => f.endsWith(".sql"))
    .sort();

  for (const file of files) {
    const sql = fs.readFileSync(path.join(migrationsDir, file), "utf-8");
    const statements = sql
      .split("--> statement-breakpoint")
      .map(s => s.trim())
      .filter(Boolean);

    for (const stmt of statements) {
      try {
        sqlite.exec(stmt);
      } catch (e: any) {
        if (!e.message?.includes("already exists") && !e.message?.includes("duplicate")) {
          throw e;
        }
      }
    }
  }

  console.log(`Migrations applied (${files.length} files)`);
}

async function createAccount(
  auth: ReturnType<typeof betterAuth>,
  email: string,
  password: string,
  name: string,
  team: string,
  label: string
) {
  try {
    await auth.api.signUpEmail({ body: { email, password, name, team } });
    console.log(`Created ${label}: ${email}`);
  } catch (e: any) {
    const msg: string = e?.message ?? String(e);
    if (msg.toLowerCase().includes("unique") || msg.toLowerCase().includes("already")) {
      console.log(`${label} already exists, skipping: ${email}`);
    } else {
      console.error(`Failed to create ${label} (${email}): ${msg}`);
    }
  }
}

async function seed() {
  const dbPath = findDbFile();
  console.log(`Using database: ${dbPath}`);

  const sqlite = new Database(dbPath);
  applyMigrations(sqlite);

  const db = drizzle(sqlite, { schema });
  const auth = betterAuth({
    database: drizzleAdapter(db, { provider: "sqlite" }),
    emailAndPassword: { enabled: true },
    user: {
      additionalFields: {
        team: { type: "string", required: true },
        role: { type: "string", required: false, defaultValue: "user", input: false },
      },
    },
  });

  await createAccount(auth, "admin@fgc.com", "Admin@fgc2026", "Admin", "Admin Team", "Admin");
  await createAccount(auth, "user@fgc.com", "User@fgc2026", "Test User", "Team Vietnam", "Test User");

  sqlite.prepare(`UPDATE user SET role = 'admin' WHERE email = 'admin@fgc.com'`).run();
  console.log("admin@fgc.com promoted to role=admin");

  sqlite.close();
  console.log("\nDone! Login credentials:");
  console.log("  admin@fgc.com  / Admin@fgc2026  (admin)");
  console.log("  user@fgc.com   / User@fgc2026   (user)");
}

seed().catch(e => { console.error(e); process.exit(1); });


