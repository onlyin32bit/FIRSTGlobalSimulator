ALTER TABLE `game_servers` ADD `runtimeJson` text;
--> statement-breakpoint
CREATE TABLE `game_server_instances` (
  `id` text PRIMARY KEY NOT NULL,
  `serverId` text NOT NULL,
  `machineId` text NOT NULL,
  `appName` text,
  `region` text,
  `privateIp` text,
  `discoveredAt` integer NOT NULL,
  `lastSeenAt` integer NOT NULL,
  FOREIGN KEY (`serverId`) REFERENCES `game_servers`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `game_server_instances_server_machine_idx` ON `game_server_instances` (`serverId`, `machineId`);
--> statement-breakpoint
CREATE TABLE `game_server_runtime_matches` (
  `id` text PRIMARY KEY NOT NULL,
  `serverId` text NOT NULL,
  `matchId` text NOT NULL,
  `players` integer NOT NULL DEFAULT 0,
  `objects` integer NOT NULL DEFAULT 0,
  `contacts` integer NOT NULL DEFAULT 0,
  `tick` integer NOT NULL DEFAULT 0,
  `tps` real NOT NULL DEFAULT 0,
  `physicsTickMs` real NOT NULL DEFAULT 0,
  `physicsLoadPercent` real NOT NULL DEFAULT 0,
  `clockDriftMs` real NOT NULL DEFAULT 0,
  `updatedAt` integer NOT NULL,
  FOREIGN KEY (`serverId`) REFERENCES `game_servers`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `game_server_runtime_matches_server_match_idx` ON `game_server_runtime_matches` (`serverId`, `matchId`);
--> statement-breakpoint
CREATE TABLE `game_server_commands` (
  `id` text PRIMARY KEY NOT NULL,
  `serverId` text NOT NULL,
  `type` text NOT NULL,
  `payload` text NOT NULL,
  `status` text NOT NULL DEFAULT 'pending',
  `error` text,
  `createdAt` integer NOT NULL,
  `deliveredAt` integer,
  `completedAt` integer,
  FOREIGN KEY (`serverId`) REFERENCES `game_servers`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `game_server_commands_pending_idx` ON `game_server_commands` (`serverId`, `status`, `createdAt`);
