<script lang="ts">
	import type { LobbySlotId, MatchLobby } from '$lib/api';
	import { cn } from '$lib/utils';
	import { FieldProjector } from './field-projector';
	import type { LobbyField } from './lobby-controller.svelte';

	type LobbySlot = MatchLobby['slots'][number];
	type FieldStation = LobbyField['layout']['stations'][number];

	let {
		field,
		lobby,
		userId,
		restrictedAlliance,
		working,
		onClaim
	}: {
		field: LobbyField;
		lobby: MatchLobby;
		userId: string;
		restrictedAlliance: string | null;
		working: string | null;
		onClaim: (slotId: LobbySlotId) => void;
	} = $props();

	const projector = $derived(new FieldProjector(field.layout.topDown));

	function stationArea(station: FieldStation) {
		const semanticArea = station.area.semanticBounds
			? field.semanticAreas[station.area.semanticBounds]
			: null;
		const point = field.anchors[station.semanticAnchor];
		if (!semanticArea && !point) return null;
		const projected = semanticArea
			? projector.projectBounds(semanticArea)
			: projector.projectFootprint(point!, station.area.widthM ?? 0.7, station.area.heightM ?? 0.7);
		return {
			left: `${projected.left}%`,
			top: `${projected.top}%`,
			width: `${projected.width}%`,
			height: `${projected.height}%`
		};
	}

	function stationClass(alliance: 'red' | 'blue', slot: LobbySlot) {
		return cn(
			'absolute z-10 grid place-items-center rounded-md border transition duration-200 motion-reduce:transition-none',
			'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white enabled:cursor-pointer enabled:hover:brightness-125 enabled:active:brightness-90 disabled:cursor-not-allowed disabled:opacity-75',
			alliance === 'red'
				? 'border-rose-300/45 bg-rose-500/10 enabled:hover:bg-rose-500/20'
				: 'border-sky-300/45 bg-sky-500/10 enabled:hover:bg-sky-500/20',
			slot.occupant?.ready && 'ring-2 ring-emerald-400 ring-offset-2 ring-offset-slate-950'
		);
	}

	function pinClass(slot: LobbySlot) {
		return cn(
			'relative inline-flex h-9 items-center rounded-full bg-slate-950/90 text-left text-white shadow-lg shadow-black/30 backdrop-blur-sm',
			slot.occupant ? 'gap-1.5 py-1 pr-3 pl-1' : 'size-9 justify-center p-1'
		);
	}

	function stationIndex(label: string) {
		return label.match(/\d+$/)?.[0] ?? 'HP';
	}

	function indexClass(alliance: 'red' | 'blue') {
		return cn(
			'grid size-7 shrink-0 place-items-center rounded-full text-xs font-bold tabular-nums',
			alliance === 'red' ? 'bg-rose-500 text-white' : 'bg-sky-500 text-white'
		);
	}

	function statusDotClass(slot: LobbySlot) {
		return cn(
			'absolute -top-0.5 -right-0.5 size-2.5 rounded-full ring-2 ring-slate-950',
			slot.occupant?.ready && 'bg-emerald-400',
			slot.occupant && !slot.occupant.ready && 'bg-amber-300',
			!slot.occupant && 'bg-slate-500'
		);
	}
</script>

<section class="overflow-hidden rounded-xl bg-card shadow-sm" aria-label="Field stations">
	<div class="relative aspect-[1282/913]">
		<img
			class="absolute inset-0 size-full object-contain"
			src={field.imageUrl}
			alt="Top-down FGC 2026 field"
		/>
		{#each field.layout.stations as station (station.slotId)}
			{@const slot = lobby.slots.find((candidate) => candidate.id === station.slotId)}
			{@const area = stationArea(station)}
			{@const alliance = station.slotId.startsWith('red-') ? 'red' : 'blue'}
			{@const selectable =
				slot &&
				(!slot.occupant || slot.occupant.userId === userId) &&
				(restrictedAlliance === null || restrictedAlliance === alliance)}
			{#if area && slot}
				<button
					type="button"
					class={stationClass(alliance, slot)}
					style:left={area.left}
					style:top={area.top}
					style:width={area.width}
					style:height={area.height}
					disabled={!selectable || working !== null}
					onclick={() => onClaim(slot.id)}
					aria-label={`${station.label}${slot.occupant ? `, ${slot.occupant.name}, ${slot.occupant.ready ? 'ready' : 'not ready'}` : ', available'}`}
				>
					<span class={pinClass(slot)}>
						<span class={indexClass(alliance)}>{stationIndex(station.label)}</span>
						{#if slot.occupant}
							<span class="max-w-28 truncate text-xs font-medium text-slate-100">
								{slot.occupant.name}
							</span>
						{/if}
						<span class={statusDotClass(slot)} aria-hidden="true"></span>
					</span>
				</button>
			{/if}
		{/each}
	</div>
</section>
