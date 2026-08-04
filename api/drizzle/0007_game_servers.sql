CREATE TABLE `game_servers` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`origin` text NOT NULL,
	`keyHash` text NOT NULL UNIQUE,
	`maxUsers` integer NOT NULL DEFAULT 50,
	`maxMatches` integer NOT NULL DEFAULT 10,
	`slots` integer NOT NULL DEFAULT 10,
	`activeUsers` integer NOT NULL DEFAULT 0,
	`activeMatches` integer NOT NULL DEFAULT 0,
	`status` text NOT NULL DEFAULT 'provisioning',
	`lastHeartbeatAt` integer,
	`createdAt` integer NOT NULL,
	`updatedAt` integer NOT NULL,
	`disabledAt` integer
);
--> statement-breakpoint
ALTER TABLE `matches` ADD `gameServerId` text REFERENCES `game_servers`(`id`);
--> statement-breakpoint
CREATE INDEX `game_servers_status_idx` ON `game_servers` (`status`);
--> statement-breakpoint
CREATE INDEX `game_servers_heartbeat_idx` ON `game_servers` (`lastHeartbeatAt`);
