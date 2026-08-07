<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { signOut, useSession } from '$lib/auth-client';
	import { api, type ApiUser } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import {
		IconCpu,
		IconDashboard,
		IconLogout,
		IconPlayerPlay,
		IconRobot,
		IconShieldCheck,
		IconWorld
	} from '@tabler/icons-svelte';

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
	<div class="flex h-screen w-full flex-col items-center justify-center gap-4 bg-background" aria-label="Checking your session">
		<div class="relative flex size-12 items-center justify-center">
			<div class="absolute size-full animate-ping rounded-full bg-primary/20"></div>
			<div class="size-8 animate-spin rounded-full border-2 border-primary border-t-transparent"></div>
		</div>
		<p class="font-mono text-xs text-primary/80 uppercase tracking-widest">INITIALIZING HUD SYSTEM...</p>
	</div>
{:else if $session.data}
	<div class="min-h-screen bg-background text-foreground selection:bg-primary selection:text-primary-foreground">
		<header class="sticky top-0 z-50 flex min-h-16 items-center justify-between border-b border-primary/20 bg-background/80 px-4 sm:px-8 backdrop-blur-xl">
			<div class="flex items-center gap-6">
				<a class="group flex items-center gap-3 font-daybreaker text-xl tracking-wider text-primary transition-all hover:text-primary/90" href="/dashboard">
					<span class="flex size-9 items-center justify-center rounded-lg border border-primary/40 bg-primary/10 shadow-[0_0_15px_rgba(234,88,12,0.25)] transition-transform group-hover:scale-105">
						<IconCpu class="size-5 text-primary" />
					</span>
					<span>FGC 2026</span>
					<span class="hidden rounded border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] tracking-normal text-emerald-400 sm:inline-block">LIVE HUD</span>
				</a>

				<nav class="hidden md:flex items-center gap-1 font-mono text-xs">
					<a href="/dashboard" class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 transition-colors hover:bg-white/5 hover:text-primary font-medium">
						<IconDashboard class="size-4 text-muted-foreground" /> HQ Command
					</a>
					<a href="/robot" class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 transition-colors hover:bg-white/5 hover:text-primary font-medium">
						<IconRobot class="size-4 text-muted-foreground" /> Hangar
					</a>
					<a href="/match/test-match" class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 transition-colors hover:bg-white/5 hover:text-primary font-medium">
						<IconPlayerPlay class="size-4 text-muted-foreground" /> Arena
					</a>
					<a href="/scene" class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 transition-colors hover:bg-white/5 hover:text-primary font-medium">
						<IconWorld class="size-4 text-muted-foreground" /> Sandbox
					</a>
				</nav>
			</div>

			<div class="flex items-center gap-3">
				<div class="hidden sm:flex items-center gap-3 rounded-full border border-border bg-card/60 px-3 py-1.5 text-xs backdrop-blur">
					<span class="size-2 rounded-full bg-emerald-500 shadow-[0_0_8px_#22c55e]"></span>
					<div class="text-right font-mono">
						<p class="font-bold leading-none text-foreground">{$session.data.user.name}</p>
						<p class="text-[10px] text-muted-foreground leading-tight">{currentUser?.team || 'Pilot'}</p>
					</div>
				</div>

				{#if currentUser?.role === 'admin'}
					<Button variant="outline" size="sm" href="/admin" class="border-amber-500/40 text-amber-400 hover:bg-amber-500/10">
						<IconShieldCheck class="size-4" /> Admin
					</Button>
				{/if}

				<Button variant="ghost" size="sm" disabled={isSigningOut} onclick={handleSignOut} class="text-muted-foreground hover:text-destructive">
					<IconLogout class="size-4" />
					<span class="hidden sm:inline">{isSigningOut ? 'Disconnecting…' : 'Sign out'}</span>
				</Button>
			</div>
		</header>
		{@render children()}
	</div>
{/if}
