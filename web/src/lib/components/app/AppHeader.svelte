<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		IconDashboard,
		IconLogout,
		IconPlayerPlay,
		IconRobot,
		IconShieldCheck,
		IconUsers
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import type { ApiUser } from '$lib/api';
	import SettingsDialog from '$lib/features/settings/SettingsDialog.svelte';

	let {
		user,
		isSigningOut,
		onSignOut,
		onUserChanged
	}: {
		user: ApiUser | null;
		isSigningOut: boolean;
		onSignOut: () => void;
		onUserChanged: (user: ApiUser) => void;
	} = $props();

	const navigation = [
		{ href: '/dashboard', label: 'Dashboard', icon: IconDashboard },
		{ href: '/robot', label: 'Robots', icon: IconRobot },
		{ href: '/match/test-match', label: 'Practice', icon: IconPlayerPlay },
		{ href: '/match/arena', label: 'Open arena', icon: IconUsers }
	] as const;

	function isActive(href: string) {
		return href === '/dashboard'
			? page.url.pathname === href
			: page.url.pathname === href || page.url.pathname.startsWith(`${href}/`);
	}
</script>

<header class="sticky top-0 z-40 bg-background/95 backdrop-blur">
	<div class="mx-auto flex min-h-16 max-w-7xl items-center gap-3 px-4 sm:px-6">
		<a class="font-daybreaker text-lg tracking-wide text-primary" href={resolve('/dashboard')}
			>FG Simulator</a
		>

		<nav class="hidden items-center gap-1 md:flex" aria-label="Game navigation">
			{#each navigation as item (item.href)}
				{@const Icon = item.icon}
				<a href={resolve(item.href)} class:nav-current={isActive(item.href)} class="nav-link">
					<Icon class="size-4" />
					{item.label}
				</a>
			{/each}
		</nav>

		<div class="ml-auto flex items-center gap-2">
			{#if user?.role === 'admin'}
				<Button variant="ghost" size="sm" href={resolve('/admin')}>
					<IconShieldCheck class="size-4" />
					<span class="hidden sm:inline">Admin</span>
				</Button>
			{/if}
			<SettingsDialog {user} {onUserChanged} />
			<Button
				variant="ghost"
				size="sm"
				disabled={isSigningOut}
				onclick={onSignOut}
				aria-label="Sign out"
			>
				<IconLogout class="size-4" />
				<span class="hidden sm:inline">{isSigningOut ? 'Signing out…' : 'Sign out'}</span>
			</Button>
		</div>
	</div>
</header>

<style>
	.nav-link {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		border-radius: 0.5rem;
		padding: 0.5rem 0.625rem;
		color: var(--muted-foreground);
		font-size: 0.875rem;
		font-weight: 500;
		transition:
			color 160ms ease,
			background-color 160ms ease;
	}

	.nav-link:hover,
	.nav-current {
		background: color-mix(in oklch, var(--primary) 12%, transparent);
		color: var(--foreground);
	}
</style>
