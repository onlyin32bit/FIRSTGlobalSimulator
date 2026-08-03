ALTER TABLE `user` ADD `disabledAt` integer;
--> statement-breakpoint
ALTER TABLE `user` ADD `disabledReason` text;
--> statement-breakpoint
ALTER TABLE `matches` ADD `updatedAt` integer NOT NULL DEFAULT 0;
--> statement-breakpoint
ALTER TABLE `matches` ADD `cancelledAt` integer;
--> statement-breakpoint
ALTER TABLE `matches` ADD `cancelReason` text;
--> statement-breakpoint
UPDATE `matches` SET `updatedAt` = `createdAt` WHERE `updatedAt` = 0;