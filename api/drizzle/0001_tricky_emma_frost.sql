CREATE TABLE `invitations` (
	`code` text PRIMARY KEY NOT NULL,
	`used` integer DEFAULT false NOT NULL,
	`createdAt` integer NOT NULL
);
--> statement-breakpoint
ALTER TABLE `user` ADD `country` text;--> statement-breakpoint
ALTER TABLE `user` ADD `team` text;