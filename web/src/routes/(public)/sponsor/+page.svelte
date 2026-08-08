<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { resolve } from '$app/paths';
</script>

<svelte:head>
	<title>Sponsor the project · FIRST Global Simulator</title>
	<meta name="description" content="Help sustain and grow the FIRST Global Simulator project." />
</svelte:head>

<div class="relative min-h-screen overflow-hidden bg-background text-foreground">
	<div
		class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_top_right,var(--color-primary)_0,transparent_45%)] opacity-10"
	></div>
	<img class="pointer-events-none fixed top-0 left-0 opacity-70" src="/images/fire-1.svg" alt="" />
	<img
		class="pointer-events-none fixed right-0 bottom-0 opacity-70"
		src="/images/fire-5.svg"
		alt=""
	/>

	<nav
		class="relative z-10 mx-auto flex w-full max-w-6xl items-center justify-between px-5 py-5 sm:px-8"
		aria-label="Public navigation"
	>
		<a class="font-semibold text-primary" href={resolve('/')}>FIRST Global Simulator</a>
		<div class="flex items-center gap-1">
			<a
				class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
				href={resolve('/')}>Home</a
			>
			<a
				class="rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
				href={resolve('/docs')}>Documentation</a
			>
		</div>
	</nav>

	<main class="relative z-10 mx-auto w-full max-w-5xl px-5 pt-12 pb-20 sm:px-8">
		<header class="max-w-3xl">
			<p class="text-sm font-semibold tracking-widest text-primary uppercase">
				Project sponsorship
			</p>
			<h1 class="mt-3 text-4xl font-bold tracking-tight sm:text-5xl">
				Help power the FIRST Global Simulator.
			</h1>
			<p class="mt-5 text-lg leading-relaxed text-muted-foreground">
				This community project needs dependable infrastructure and technical partners to keep a
				robotics simulator accessible, reliable, and ready to grow with its global users.
			</p>
			<div class="mt-8 flex flex-wrap gap-3">
				<Button
					href="mailto:vietnamteamfgc@gmail.com?subject=FIRST%20Global%20Simulator%20project%20sponsorship"
					>Discuss sponsorship</Button
				><Button variant="outline" href={resolve('/docs')}>Read the project docs</Button>
			</div>
		</header>

		<section class="mt-14 grid gap-5 md:grid-cols-2">
			<article class="rounded-xl border border-border bg-card/70 p-6">
				<h2 class="text-xl font-semibold">Infrastructure</h2>
				<p class="mt-3 leading-relaxed text-muted-foreground">
					Support web and API hosting, persistent services, databases, storage, bandwidth,
					observability, and security tooling that keep the project available to its community.
				</p>
			</article>
			<article class="rounded-xl border border-border bg-card/70 p-6">
				<h2 class="text-xl font-semibold">Compute and services</h2>
				<p class="mt-3 leading-relaxed text-muted-foreground">
					Cloud credits, compute capacity, development environments, and platform services help
					advance real-time simulation and future multiplayer testing.
				</p>
			</article>
			<article class="rounded-xl border border-border bg-card/70 p-6">
				<h2 class="text-xl font-semibold">Hardware and expertise</h2>
				<p class="mt-3 leading-relaxed text-muted-foreground">
					Equipment, engineering guidance, technical mentorship, and introductions to aligned
					partners can directly strengthen the simulator and its maintainers.
				</p>
			</article>
			<article class="rounded-xl border border-border bg-card/70 p-6">
				<h2 class="text-xl font-semibold">What support enables</h2>
				<p class="mt-3 leading-relaxed text-muted-foreground">
					A more dependable simulator, broader access for students and robotics communities, and
					faster progress toward richer game and multiplayer capabilities.
				</p>
			</article>
		</section>

		<section class="mt-12 rounded-xl border border-primary/25 bg-primary/10 p-6 sm:p-8">
			<h2 class="text-2xl font-semibold">Partner with the project</h2>
			<p class="mt-3 max-w-3xl leading-relaxed text-muted-foreground">
				We welcome conversations about hosting, credits, services, hardware, technical expertise,
				and collaboration. The project maintainers can discuss a support scope, acknowledgement, and
				any technical integration directly with your organization.
			</p>
			<div class="mt-6 flex flex-wrap gap-4 text-sm">
				<a
					class="font-medium text-primary underline underline-offset-4"
					href="mailto:vietnamteamfgc@gmail.com?subject=FIRST%20Global%20Simulator%20project%20sponsorship"
					>vietnamteamfgc@gmail.com</a
				><a
					class="font-medium text-primary underline underline-offset-4"
					href="https://www.facebook.com/TeamVietnamFGC">Team Vietnam on Facebook</a
				><a
					class="font-medium text-primary underline underline-offset-4"
					href="https://www.instagram.com/teamvietnam.fgc/">Team Vietnam on Instagram</a
				>
			</div>
		</section>

		<p class="mt-10 text-xs leading-relaxed text-muted-foreground">
			FIRST Global Simulator is a Team Vietnam community project and is not affiliated with,
			sponsored by, or endorsed by FIRST® or FIRST® Global.
		</p>
	</main>
</div>
