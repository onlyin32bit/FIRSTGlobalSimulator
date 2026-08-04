<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { signOut, useSession } from '$lib/auth-client';
	import { api, type ApiUser } from '$lib/api';
	import { Button } from '$lib/components/ui/button';

	let { children } = $props();
	const session = useSession();
	let currentUser = $state<ApiUser | null>(null);
	let isSigningOut = $state(false);

	onMount(async () => {
		try {
			currentUser = (await api.getCurrentUser()).user;
		} catch {
			currentUser = null;
		}
	});

	$effect(() => {
		if (!$session.data) currentUser = null;
	});

	$effect(() => {
		if (!$session.isPending && !$session.data) {
			const next = `${page.url.pathname}${page.url.search}`;
			goto(`/auth?next=${encodeURIComponent(next)}`);
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

{#if $session.isPending}
	<div class="flex h-screen w-full items-center justify-center" aria-label="Checking your session">
		<div
			class="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent"
		></div>
	</div>
{:else if $session.data}
	<div class="min-h-screen bg-background">
		<header class="flex min-h-14 items-center justify-between border-b border-border px-4 sm:px-6">
			<a class="font-semibold text-primary" href="/dashboard">FGC 2026</a>
			<div class="flex items-center gap-3 text-right">
				<div class="hidden text-sm sm:block">
					<p class="font-medium">{$session.data.user.name}</p>
					<p class="text-xs text-muted-foreground">{currentUser?.team || 'Simulator member'}</p>
				</div>
				{#if currentUser?.role === 'admin'}
					<Button variant="ghost" size="sm" href="/admin">Admin</Button>
				{/if}
				<Button variant="ghost" size="sm" disabled={isSigningOut} onclick={handleSignOut}>
					{isSigningOut ? 'Signing out…' : 'Sign out'}
				</Button>
			</div>
		</header>
		{@render children()}
	</div>
{/if}
