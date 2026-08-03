<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import RequestForm from './request-form.svelte';
	import { resolve } from '$app/paths';

	let isInviteDialogOpen = $state(false);
</script>

<div
	class="relative flex h-screen w-full flex-col items-center justify-center overflow-hidden bg-background text-center"
>
	<div
		class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_center,var(--color-primary)_0,transparent_70%)] opacity-[0.03]"
	></div>

	<img class="fixed top-0 left-0" src="/images/fire-1.svg" alt="" />
	<img class="fixed top-1/2 left-0" src="/images/fire-2.svg" alt="" />
	<img class="fixed bottom-0 left-0" src="/images/fire-3.svg" alt="" />
	<img class="fixed right-0 bottom-1/2" src="/images/fire-4.svg" alt="" />
	<img class="fixed right-0 bottom-0" src="/images/fire-5.svg" alt="" />
	<img class="fixed -top-10 right-1/4 scale-150 rotate-90" src="/images/fire-2.svg" alt="" />

	<nav
		class="absolute top-0 right-0 left-0 z-20 flex items-center justify-end gap-1 p-4 sm:gap-2 sm:p-6"
		aria-label="Public navigation"
	>
		<a
			class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
			href={resolve('/docs')}>Documentation</a
		>
		<a
			class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
			href={resolve('/sponsor')}>Sponsor</a
		>
	</nav>

	<div class="z-10 flex max-w-2xl flex-col items-center gap-6 px-4">
		<h1 class="font-daybreaker text-6xl font-black tracking-wide text-primary">
			FIRST Global Simulator by Team Vietnam
		</h1>
		<p class="text-xl leading-relaxed font-medium text-muted-foreground">
			Join Team Vietnam and the global robotics community to design, build, and simulate your 2026
			Igniting Innovation robot in a fully synchronized, physics-driven multiplayer environment.
		</p>

		<div class="mt-8 flex gap-4">
			<Button href="/dashboard" size="lg" class="px-8 text-lg font-bold">Enter Simulator</Button>
			<Button
				variant="outline"
				size="lg"
				class="px-8 text-lg font-bold"
				onclick={() => (isInviteDialogOpen = true)}>Request Invite</Button
			>
		</div>

		<div class="mt-16 flex w-full max-w-lg flex-col items-center gap-6">
			<div
				class="flex cursor-default items-center justify-center gap-8 opacity-50 grayscale transition-all hover:opacity-100 hover:grayscale-0"
			>
				<img src="/images/first-global.webp" alt="FIRST Global Logo" class="h-12 object-contain" />
				<img
					src="/images/first-global-2026.png"
					alt="FIRST Global 2026 Logo"
					class="h-20 object-contain"
				/>
				<img
					src="/images/team-vietnam.svg"
					alt="Team Vietnam Logo"
					class="h-14 rounded-md object-contain"
				/>
			</div>
			<div
				class="flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-sm font-medium text-muted-foreground"
			>
				<a
					class="transition-colors hover:text-primary"
					href="https://www.facebook.com/TeamVietnamFGC">Facebook</a
				>
				<a
					class="transition-colors hover:text-primary"
					href="https://www.instagram.com/teamvietnam.fgc/">Instagram</a
				>
				<a class="transition-colors hover:text-primary" href="mailto:vietnamteamfgc@gmail.com"
					>vietnamteamfgc@gmail.com</a
				>
			</div>
			<p class="text-xs font-medium text-muted-foreground">
				FIRST Global Simulator is a Team Vietnam community project and is <strong>NOT</strong>
				affiliated with, sponsored by, or endorsed by <em>FIRST</em>® or <em>FIRST</em>® Global.
			</p>
		</div>
	</div>

	<RequestForm bind:isOpen={isInviteDialogOpen} />
</div>
