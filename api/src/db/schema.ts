import { sqliteTable, text, integer, real } from 'drizzle-orm/sqlite-core';

export const user = sqliteTable("user", {
	id: text("id").primaryKey(),
	name: text("name").notNull(),
	email: text("email").notNull().unique(),
	emailVerified: integer("emailVerified", { mode: "boolean" }).notNull(),
	image: text("image"),
	team: text("team"),
	role: text("role").notNull().default("user"),
	disabledAt: integer("disabledAt", { mode: "timestamp" }),
	disabledReason: text("disabledReason"),
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
  updatedAt: integer('updatedAt', { mode: 'timestamp' }).notNull(),
  cancelledAt: integer('cancelledAt', { mode: 'timestamp' }),
  cancelReason: text('cancelReason'),
  gameServerId: text('gameServerId').references(() => gameServers.id),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
});

export const gameServers = sqliteTable('game_servers', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  origin: text('origin').notNull(),
  keyHash: text('keyHash').notNull().unique(),
  maxUsers: integer('maxUsers').notNull().default(50),
  maxMatches: integer('maxMatches').notNull().default(10),
  slots: integer('slots').notNull().default(10),
  activeUsers: integer('activeUsers').notNull().default(0),
  activeMatches: integer('activeMatches').notNull().default(0),
  status: text('status').notNull().default('provisioning'),
  lastHeartbeatAt: integer('lastHeartbeatAt', { mode: 'timestamp' }),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
  updatedAt: integer('updatedAt', { mode: 'timestamp' }).notNull(),
  disabledAt: integer('disabledAt', { mode: 'timestamp' }),
  runtimeJson: text('runtimeJson'),
});

export const gameServerInstances = sqliteTable('game_server_instances', {
  id: text('id').primaryKey(),
  serverId: text('serverId').notNull().references(() => gameServers.id),
  machineId: text('machineId').notNull(),
  appName: text('appName'),
  region: text('region'),
  privateIp: text('privateIp'),
  discoveredAt: integer('discoveredAt', { mode: 'timestamp' }).notNull(),
  lastSeenAt: integer('lastSeenAt', { mode: 'timestamp' }).notNull(),
});

export const gameServerRuntimeMatches = sqliteTable('game_server_runtime_matches', {
  id: text('id').primaryKey(),
  serverId: text('serverId').notNull().references(() => gameServers.id),
  matchId: text('matchId').notNull(),
  players: integer('players').notNull().default(0),
  objects: integer('objects').notNull().default(0),
  contacts: integer('contacts').notNull().default(0),
  tick: integer('tick').notNull().default(0),
  tps: real('tps').notNull().default(0),
  physicsTickMs: real('physicsTickMs').notNull().default(0),
  physicsLoadPercent: real('physicsLoadPercent').notNull().default(0),
  clockDriftMs: real('clockDriftMs').notNull().default(0),
  updatedAt: integer('updatedAt', { mode: 'timestamp' }).notNull(),
});

export const gameServerCommands = sqliteTable('game_server_commands', {
  id: text('id').primaryKey(),
  serverId: text('serverId').notNull().references(() => gameServers.id),
  type: text('type').notNull(),
  payload: text('payload').notNull(),
  status: text('status').notNull().default('pending'),
  error: text('error'),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
  deliveredAt: integer('deliveredAt', { mode: 'timestamp' }),
  completedAt: integer('completedAt', { mode: 'timestamp' }),
});

export const invitations = sqliteTable('invitations', {
  code: text('code').primaryKey(),
  used: integer('used', { mode: 'boolean' }).notNull().default(false),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
  expiresAt: integer('expiresAt', { mode: 'timestamp' }),
  revokedAt: integer('revokedAt', { mode: 'timestamp' }),
  redeemedAt: integer('redeemedAt', { mode: 'timestamp' }),
  redeemedByUserId: text('redeemedByUserId').references(() => user.id),
});

export const adminAuditLog = sqliteTable('admin_audit_log', {
  id: text('id').primaryKey(),
  actorUserId: text('actorUserId').notNull().references(() => user.id),
  action: text('action').notNull(),
  targetType: text('targetType').notNull(),
  targetId: text('targetId').notNull(),
  metadata: text('metadata'),
  createdAt: integer('createdAt', { mode: 'timestamp' }).notNull(),
});
