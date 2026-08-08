import type { Pathname } from '$app/types';
import type { Icon } from '@tabler/icons-svelte';
import {
	IconActivityHeartbeat,
	IconBox,
	IconClipboardList,
	IconHome,
	IconKey,
	IconServer,
	IconUsers
} from '@tabler/icons-svelte';

export type AdminNavItem = {
	title: string;
	href: Pathname;
	icon: Icon;
};

export const adminNav: AdminNavItem[] = [
	{ title: 'Home', href: '/admin', icon: IconHome },
	{ title: 'Users', href: '/admin/users', icon: IconUsers },
	{ title: 'Invitations', href: '/admin/invitations', icon: IconKey },
	{ title: 'Matches', href: '/admin/matches', icon: IconActivityHeartbeat },
	{ title: 'Game packs', href: '/admin/game-packs', icon: IconBox },
	{ title: 'Game servers', href: '/admin/game-servers', icon: IconServer },
	{ title: 'Audit log', href: '/admin/audit-log', icon: IconClipboardList }
];
