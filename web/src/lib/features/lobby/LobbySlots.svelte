<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { LobbySlotId, MatchLobby } from '$lib/api';

	let {
		slots,
		alliance,
		userId,
		restrictedAlliance,
		working,
		onClaim
	}: {
		slots: MatchLobby['slots'];
		alliance: 'red' | 'blue';
		userId: string;
		restrictedAlliance: string | null;
		working: string | null;
		onClaim: (slotId: LobbySlotId) => void;
	} = $props();

	const label = $derived(alliance === 'red' ? 'Red alliance' : 'Blue alliance');
	const isRestricted = $derived(restrictedAlliance !== null && restrictedAlliance !== alliance);
</script>

<section aria-label={label} class="rounded-xl bg-card p-4 shadow-sm">
	<h2 class="text-sm font-semibold">{label}</h2>
	<div class="mt-3 space-y-2">
		{#each slots.filter((slot) => slot.alliance === alliance) as slot (slot.id)}
			{@const mine = slot.occupant?.userId === userId}
			<div class="flex min-h-16 items-center gap-3 rounded-lg bg-muted/65 px-3 py-2">
				<div class="min-w-0 flex-1">
					<p class="text-sm font-medium">{slot.label}</p>
					<p class="truncate text-xs text-muted-foreground">
						{slot.occupant ? slot.occupant.name : 'Available'}
					</p>
				</div>
				{#if slot.occupant}
					<span class="text-xs text-muted-foreground"
						>{slot.occupant.ready ? 'Ready' : 'Not ready'}</span
					>
				{/if}
				{#if (!slot.occupant || mine) && !isRestricted}
					<Button
						variant={mine ? 'secondary' : 'ghost'}
						size="sm"
						disabled={working !== null}
						onclick={() => onClaim(slot.id)}
					>
						{working === slot.id ? '…' : mine ? 'Selected' : 'Join'}
					</Button>
				{/if}
			</div>
		{/each}
	</div>
</section>
