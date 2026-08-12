ALTER TABLE `matches` ADD `matchSeed` integer;
--> statement-breakpoint
ALTER TABLE `matches` ADD `packVersion` text;
--> statement-breakpoint
ALTER TABLE `matches` ADD `startsAt` integer;
--> statement-breakpoint
ALTER TABLE `matches` ADD `completedAt` integer;
--> statement-breakpoint
ALTER TABLE `matches` ADD `completionReason` text;
--> statement-breakpoint
ALTER TABLE `matches` ADD `resultJson` text;
--> statement-breakpoint
ALTER TABLE `matches` ADD `replayKey` text;
--> statement-breakpoint
CREATE TABLE `match_events` (
  `id` text PRIMARY KEY NOT NULL,
  `matchId` text NOT NULL,
  `eventId` text NOT NULL,
  `tick` integer NOT NULL,
  `kind` text NOT NULL,
  `payloadJson` text NOT NULL,
  `gamePackVersion` text NOT NULL,
  `createdAt` integer NOT NULL,
  FOREIGN KEY (`matchId`) REFERENCES `matches`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `match_events_match_event_idx` ON `match_events` (`matchId`, `eventId`);
--> statement-breakpoint
CREATE INDEX `match_events_match_tick_idx` ON `match_events` (`matchId`, `tick`);
