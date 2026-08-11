<script lang="ts">
	import type { Robot } from '$lib/api';
	let { robots, isLoading }: { robots: Robot[]; isLoading: boolean } = $props();
</script>

<section aria-labelledby="saved-robots">
	<h2 id="saved-robots" class="text-lg font-semibold">Saved robots</h2>
	{#if isLoading}
		<p class="mt-3 text-sm text-muted-foreground">Loading saved robots…</p>
	{:else if robots.length === 0}
		<p class="mt-3 text-sm text-muted-foreground">Save a build to use it in a match lobby.</p>
	{:else}
		<ul class="mt-3 space-y-1">
			{#each robots as robot (robot.id)}
				<li class="flex items-center justify-between rounded-lg bg-muted/60 px-3 py-2 text-sm">
					<span class="font-medium">{robot.name}</span>
					<time class="text-xs text-muted-foreground"
						>{new Date(robot.updatedAt).toLocaleDateString()}</time
					>
				</li>
			{/each}
		</ul>
	{/if}
</section>
