import { sqliteTable, text, integer } from 'drizzle-orm/sqlite-core';

export const user = sqliteTable("user", {
	id: text("id").primaryKey(),
	name: text("name").notNull(),
	email: text("email").notNull().unique(),
	emailVerified: integer("emailVerified", { mode: "boolean" }).notNull(),
	image: text("image"),
	team: text("team"),
	role: text("role").notNull().default("user"),
	createdAt: integer("createdAt", { mode: "timestamp" }).notNull(),
	updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull()
});

export const session = sqliteTable("session", {
	id: text("id").primaryKey(),
	expiresAt: integer("expiresAt", { mode: "timestamp" }).notNull(),
	token: text("token").notNull().unique(),
	createdAt: integer("createdAt", { mode: "timestamp" }).notNull(),
	updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull(),
	ipAddress: text("ipAddress"),
	userAgent: text("userAgent"),
	userId: text("userId").notNull().references(() => user.id)
});

export const account = sqliteTable("account", {
	id: text("id").primaryKey(),
	accountId: text("accountId").notNull(),
	providerId: text("providerId").notNull(),
	userId: text("userId").notNull().references(() => user.id),
	accessToken: text("accessToken"),
	refreshToken: text("refreshToken"),
	idToken: text("idToken"),
	accessTokenExpiresAt: integer("accessTokenExpiresAt", { mode: "timestamp" }),
	refreshTokenExpiresAt: integer("refreshTokenExpiresAt", { mode: "timestamp" }),
	scope: text("scope"),
	password: text("password"),
	createdAt: integer("createdAt", { mode: "timestamp" }).notNull(),
	updatedAt: integer("updatedAt", { mode: "timestamp" }).notNull()
});

export const verification = sqliteTable("verification", {
	id: text("id").primaryKey(),
	identifier: text("identifier").notNull(),
	value: text("value").notNull(),
	expiresAt: integer("expiresAt", { mode: "timestamp" }).notNull(),
	createdAt: integer("createdAt", { mode: "timestamp" }),
	updatedAt: integer("updatedAt", { mode: "timestamp" })
});

// App specific tables

export const robots = sqliteTable('robots', {
  id: text('id').primaryKey(),
  userId: text('userId').notNull().references(() => user.id),
  name: text('name').notNull(),
  buildData: text('buildData').notNull(), // JSON string representing the robot modules
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updatedAt', { mode: 'timestamp' }).notNull(),
});

export const matches = sqliteTable('matches', {
  id: text('id').primaryKey(),
  hostId: text('hostId').notNull().references(() => user.id),
  gamePackId: text('gamePackId').notNull(),
  status: text('status').notNull(), // "LOBBY", "IN_PROGRESS", "FINISHED"
  maxPlayers: integer('maxPlayers').notNull(),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
});

export const invitations = sqliteTable('invitations', {
  code: text('code').primaryKey(),
  used: integer('used', { mode: 'boolean' }).notNull().default(false),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
});
