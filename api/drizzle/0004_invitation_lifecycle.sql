ALTER TABLE `invitations` ADD `expiresAt` integer;--> statement-breakpoint
ALTER TABLE `invitations` ADD `revokedAt` integer;--> statement-breakpoint
ALTER TABLE `invitations` ADD `redeemedAt` integer;--> statement-breakpoint
ALTER TABLE `invitations` ADD `redeemedByUserId` text REFERENCES `user`(`id`);