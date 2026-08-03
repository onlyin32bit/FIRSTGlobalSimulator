<script lang="ts">
	import { onMount } from 'svelte';
	import { IconActivity, IconArrowRight, IconBell, IconBox, IconUsers } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import { ApiError, api } from '$lib/api';

	let data = $state<Awaited<ReturnType<typeof api.getAdminOverview>> | null>(null);
	let error = $state('');
	const metrics = $derived(
		data
			? [
					{ label: 'Users', value: data.metrics.users, icon: IconUsers },
					{ label: 'Administrators', value: data.metrics.admins, icon: IconUsers },
					{ label: 'Saved robots', value: data.metrics.robots, icon: IconBox },
					{ label: 'Matches', value: data.metrics.matches, icon: IconActivity },
					{ label: 'Active invites', value: data.metrics.activeInvitations, icon: IconBell }
				]
			: []
	);

	onMount(async () => {
		try {
			data = await api.getAdminOverview();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load control center.';
		}
	});
</script>

<div class="mx-auto max-w-[104rem] space-y-9">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-semibold tracking-tight">Home</h1>
		<div class="hidden rounded-full bg-muted p-1 text-xs sm:flex">
			<span class="rounded-full bg-card px-3 py-1.5 font-medium shadow-sm">Live</span>
			<span class="px-3 py-1.5 text-muted-foreground">Overview</span>
		</div>
	</div>
	{#if error}
		<p
			class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive"
		>
			{error}
		</p>
	{:else if !data}
		<p class="text-sm text-muted-foreground">Loading dashboard…</p>
	{:else}
		<section
			class="relative overflow-hidden rounded-xl border border-border bg-card px-6 py-7 sm:px-8"
		>
			<div
				class="pointer-events-none absolute top-0 right-0 h-full w-2/5 bg-[radial-gradient(circle_at_70%_15%,color-mix(in_oklch,var(--primary)_20%,transparent),transparent_65%)]"
			></div>
			<div class="relative max-w-2xl">
				<div
					class="flex size-9 items-center justify-center rounded-lg border border-border bg-muted"
				>
					<IconActivity class="size-5" />
				</div>
				<h2 class="mt-5 text-xl font-semibold">Simulator operations</h2>
				<p class="mt-2 text-sm leading-6 text-muted-foreground">
					Monitor accounts, invitations, game packs, and lobby activity from one operational
					workspace.
				</p>
				<Button href="/admin/invitations" size="sm" class="mt-5"
					>Manage invitations <IconArrowRight class="ml-1 size-3.5" /></Button
				>
			</div>
		</section>
		<section class="overflow-hidden rounded-xl border border-border bg-card">
			<div class="grid sm:grid-cols-2 xl:grid-cols-5">
				{#each metrics as metric, index}
					{@const MetricIcon = metric.icon}
					<div
						class="min-h-31 border-b border-border p-5 sm:border-r xl:border-b-0 {index === 4
							? 'xl:border-r-0'
							: ''}"
					>
						<div class="flex items-center justify-between text-sm text-muted-foreground">
							<span>{metric.label}</span><MetricIcon class="size-4" />
						</div>
						<p class="mt-4 text-2xl font-semibold tracking-tight">{metric.value}</p>
					</div>
				{/each}
			</div>
		</section>
		<div class="grid gap-9 xl:grid-cols-[minmax(0,1fr)_22rem]">
			<section>
				<div class="mb-4 flex items-center justify-between">
					<h2 class="font-semibold">Recent activity</h2>
					<a class="text-sm text-muted-foreground hover:text-foreground" href="/admin/audit-log"
						>View all</a
					>
				</div>
				<div class="rounded-xl border border-border bg-card">
					{#if data.recentActivity.length === 0}<div class="p-6 text-sm text-muted-foreground">
							Administrative activity will appear here.
						</div>{:else}<ul class="divide-y divide-border">
							{#each data.recentActivity as item}<li class="flex gap-3 p-4">
									<span
										class="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-muted"
										><IconActivity class="size-3.5" /></span
									>
									<div class="min-w-0">
										<p class="text-sm font-medium">{item.action}</p>
										<p class="mt-0.5 text-xs text-muted-foreground">
											{item.actorName} · {item.targetType}: {item.targetId}
										</p>
									</div>
								</li>{/each}
						</ul>{/if}
				</div>
			</section>
			<section>
				<div class="mb-4 flex items-center justify-between">
					<h2 class="font-semibold">Recent matches</h2>
					<a class="text-sm text-muted-foreground hover:text-foreground" href="/admin/matches"
						>View all</a
					>
				</div>
				<div class="rounded-xl border border-border bg-card">
					{#if data.recentMatches.length === 0}<div class="p-6 text-sm text-muted-foreground">
							No matches created yet.
						</div>{:else}<ul class="divide-y divide-border">
							{#each data.recentMatches as match}<li class="p-4">
									<div class="flex items-center justify-between">
										<p class="text-sm font-medium">{match.gamePackId}</p>
										<span class="rounded-full bg-muted px-2 py-0.5 text-[11px]">{match.status}</span
										>
									</div>
									<p class="mt-1 text-xs text-muted-foreground">Hosted by {match.hostName}</p>
								</li>{/each}
						</ul>{/if}
				</div>
			</section>
		</div>
	{/if}
</div>
