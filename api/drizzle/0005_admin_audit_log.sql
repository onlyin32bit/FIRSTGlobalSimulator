CREATE TABLE `admin_audit_log` (
	`id` text PRIMARY KEY NOT NULL,
	`actorUserId` text NOT NULL,
	`action` text NOT NULL,
	`targetType` text NOT NULL,
	`targetId` text NOT NULL,
	`metadata` text,
	`createdAt` integer NOT NULL,
	FOREIGN KEY (`actorUserId`) REFERENCES `user`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE INDEX `admin_audit_log_createdAt_idx` ON `admin_audit_log` (`createdAt`);
--> statement-breakpoint
CREATE INDEX `admin_audit_log_actorUserId_idx` ON `admin_audit_log` (`actorUserId`);