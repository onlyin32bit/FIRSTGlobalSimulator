<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import {
		IconChevronDown,
		IconLayoutSidebarLeftCollapse,
		IconLogout,
		IconMenu2,
		IconSearch,
		IconX
	} from '@tabler/icons-svelte';
	import { signOut } from '$lib/auth-client';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { ApiError, api, type ApiUser } from '$lib/api';
	import { adminNav } from './admin-nav';

	let { children } = $props();
	let currentUser = $state<ApiUser | null>(null);
	let isLoading = $state(true);
	let isSigningOut = $state(false);
	let errorMessage = $state('');
	let mobileNavOpen = $state(false);
	let navSearch = $state('');

	const visibleNav = $derived(
		adminNav.filter((item) => item.title.toLowerCase().includes(navSearch.trim().toLowerCase()))
	);

	onMount(async () => {
		try {
			currentUser = (await api.getCurrentUser()).user;
			if (currentUser.role !== 'admin')
				errorMessage = 'Your account does not have administrator access.';
		} catch (error) {
			if (error instanceof ApiError && error.status === 401) {
				await goto(`/auth?next=${encodeURIComponent(page.url.pathname)}`);
				return;
			}
			errorMessage =
				error instanceof Error ? error.message : 'Unable to verify administrator access.';
		} finally {
			isLoading = false;
		}
	});

	async function handleSignOut() {
		isSigningOut = true;
		try {
			await signOut();
			await goto('/');
		} finally {
			isSigningOut = false;
		}
	}
</script>

{#snippet navigation()}
	<div class="flex h-full flex-col">
		<div class="flex items-center justify-between px-4 pt-5 pb-4">
			<button class="flex items-center gap-1 text-sm font-semibold text-foreground" type="button">
				Control center <IconChevronDown class="size-3.5 text-muted-foreground" />
			</button>
			<IconLayoutSidebarLeftCollapse class="size-4 text-muted-foreground" />
		</div>
		<div class="px-3">
			<div class="relative">
				<IconSearch
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					class="h-9 border-border bg-card pl-9 text-sm shadow-none"
					placeholder="Search"
					bind:value={navSearch}
				/>
			</div>
		</div>
		<nav class="mt-3 flex-1 px-3" aria-label="Admin navigation">
			{#each visibleNav as item}
				<a
					class="mb-0.5 flex h-9 items-center gap-3 rounded-md px-2.5 text-sm transition-colors {page
						.url.pathname === item.href
						? 'bg-muted font-medium text-foreground'
						: 'text-foreground/80 hover:bg-muted'}"
					href={item.href}
					onclick={() => (mobileNavOpen = false)}
				>
					<item.icon class="size-4 shrink-0" />
					<span>{item.title}</span>
				</a>
			{/each}
		</nav>
		<div class="border-t border-border p-3">
			<a
				class="flex items-center gap-3 rounded-md px-2.5 py-2 text-sm text-foreground/80 hover:bg-muted"
				href="/dashboard"><IconX class="size-4" /> Simulator</a
			>
			<div class="mt-2 flex items-center gap-2 px-2.5 py-2">
				<span
					class="flex size-6 items-center justify-center rounded-full bg-foreground text-xs font-semibold text-background"
					>{currentUser?.name.slice(0, 1).toUpperCase()}</span
				>
				<div class="min-w-0 flex-1">
					<p class="truncate text-xs font-medium">{currentUser?.name}</p>
					<p class="truncate text-[11px] text-muted-foreground">Administrator</p>
				</div>
				<button
					class="text-muted-foreground hover:text-foreground"
					type="button"
					disabled={isSigningOut}
					onclick={handleSignOut}
					><IconLogout class="size-4" /><span class="sr-only">Sign out</span></button
				>
			</div>
		</div>
	</div>
{/snippet}

<div class="admin-dashboard min-h-screen bg-background text-foreground">
	{#if isLoading}
		<div class="flex min-h-screen items-center justify-center">
			<div
				class="size-5 animate-spin rounded-full border-2 border-primary border-t-transparent"
			></div>
		</div>
	{:else if errorMessage}
		<div class="mx-auto flex min-h-screen max-w-xl flex-col items-start justify-center gap-5 p-6">
			<h1 class="text-3xl font-semibold">Access denied</h1>
			<p class="text-muted-foreground">{errorMessage}</p>
			<Button href="/dashboard">Return to simulator</Button>
		</div>
	{:else if currentUser?.role === 'admin'}
		<div class="min-h-screen lg:grid lg:grid-cols-[16rem_minmax(0,1fr)]">
			<aside class="hidden border-r border-border bg-sidebar lg:block">
				{@render navigation()}
			</aside>
			<div class="min-w-0">
				<header class="flex h-14 items-center border-b border-border bg-card px-4 lg:hidden">
					<Button variant="ghost" size="icon-sm" onclick={() => (mobileNavOpen = true)}
						><IconMenu2 /><span class="sr-only">Open navigation</span></Button
					><span class="ml-2 text-sm font-semibold">Control center</span>
				</header>
				{#if mobileNavOpen}<div
						class="fixed inset-0 z-50 bg-foreground/20 lg:hidden"
						role="presentation"
						onclick={() => (mobileNavOpen = false)}
					>
						<aside
							class="h-full w-72 bg-sidebar shadow-xl"
							onclick={(event) => event.stopPropagation()}
						>
							{@render navigation()}
						</aside>
					</div>{/if}
				<main class="min-w-0 bg-background px-5 py-7 sm:px-8 lg:px-12 lg:py-10">
					{@render children()}
				</main>
			</div>
		</div>
	{/if}
</div>

<style>
	.admin-dashboard {
		--background: oklch(0.99 0 0);
		--foreground: oklch(0.2 0.005 260);
		--card: oklch(1 0 0);
		--card-foreground: oklch(0.2 0.005 260);
		--popover: oklch(1 0 0);
		--popover-foreground: oklch(0.2 0.005 260);
		--primary: oklch(0.58 0.19 50);
		--primary-foreground: oklch(1 0 0);
		--secondary: oklch(0.96 0.002 260);
		--secondary-foreground: oklch(0.2 0.005 260);
		--muted: oklch(0.95 0.002 260);
		--muted-foreground: oklch(0.5 0.01 260);
		--accent: oklch(0.94 0.005 260);
		--accent-foreground: oklch(0.2 0.005 260);
		--destructive: oklch(0.58 0.21 25);
		--destructive-foreground: oklch(1 0 0);
		--border: oklch(0.9 0.003 260);
		--input: oklch(0.88 0.003 260);
		--ring: oklch(0.58 0.19 50);
		--sidebar: oklch(0.965 0.002 260);
		--sidebar-foreground: oklch(0.2 0.005 260);
	}
</style>
