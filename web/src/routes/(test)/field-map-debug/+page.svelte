<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { FieldProjector } from '$lib/features/lobby/field-projector';

	type MapData = {
		imageUrl: string;
		anchors: Record<string, [number, number, number]>;
		semanticAreas: Record<string, { min: [number, number, number]; max: [number, number, number] }>;
		stations: Array<{
			slotId: string;
			label: string;
			semanticAnchor: string;
			area: { widthM?: number; heightM?: number; semanticBounds?: string };
		}>;
		topDown: {
			worldBounds: { min: [number, number]; max: [number, number] };
			horizontalDirection: 'inverted' | 'normal';
			verticalDirection: 'inverted' | 'normal';
		};
	};

	let map = $state<MapData | null>(null);
	let error = $state('');
	let showLabels = $state(true);
	let stationOnly = $state(false);
	let selectedStationId = $state('red-driver-1');
	let footprintOverrides = $state<Record<string, { widthM: number; heightM: number }>>({});
	const projector = $derived(map ? new FieldProjector(map.topDown) : null);
	const selectedStation = $derived(
		map?.stations.find((station) => station.slotId === selectedStationId)
	);
	const stationAnchors = $derived(
		new Set(map?.stations.map((station) => station.semanticAnchor) ?? [])
	);
	const points = $derived(
		Object.entries(map?.anchors ?? {})
			.map(([name, point]) => ({
				name,
				point,
				station: stationAnchors.has(name),
				position: projector?.project(point)
			}))
			.filter((item) => item.position && (!stationOnly || item.station))
	);
	const stationAreas = $derived(
		map && projector
			? map.stations
					.map((station) => {
						const bounds = station.area.semanticBounds
							? map.semanticAreas[station.area.semanticBounds]
							: null;
						const anchor = map.anchors[station.semanticAnchor];
						if (!bounds && !anchor) return null;
						const footprint = footprintFor(station);
						return {
							name: station.label,
							position: bounds
								? projector.projectBounds(bounds)
								: projector.projectFootprint(anchor!, footprint.widthM, footprint.heightM)
						};
					})
					.filter(
						(
							area
						): area is { name: string; position: ReturnType<FieldProjector['projectBounds']> } =>
							area !== null
					)
			: []
	);

	function footprintFor(station: MapData['stations'][number]) {
		return (
			footprintOverrides[station.slotId] ?? {
				widthM: station.area.widthM ?? 0.7,
				heightM: station.area.heightM ?? 0.7
			}
		);
	}

	function setFootprint(axis: 'widthM' | 'heightM', value: number) {
		if (!selectedStation || !Number.isFinite(value) || value <= 0) return;
		footprintOverrides[selectedStation.slotId] = {
			...footprintFor(selectedStation),
			[axis]: value
		};
	}

	onMount(async () => {
		try {
			const [metadata, assets] = await Promise.all([
				api.getGamePackMetadata('fgc-2026'),
				api.getGamePackAssets('fgc-2026')
			]);
			if (!metadata.manifest.lobby || !assets.ui?.lobbyField) {
				throw new Error('The pack has no lobby field layout.');
			}
			map = {
				imageUrl: assets.ui.lobbyField,
				anchors: metadata.fieldDefinition.anchors,
				semanticAreas: metadata.fieldDefinition.semanticAreas,
				stations: metadata.manifest.lobby.stations,
				topDown: metadata.manifest.lobby.topDown
			};
			footprintOverrides = Object.fromEntries(
				metadata.manifest.lobby.stations
					.filter((station) => !station.area.semanticBounds)
					.map((station) => [station.slotId, footprintFor(station)])
			);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load field map data.';
		}
	});
</script>

<svelte:head><title>Field map debug</title></svelte:head>

<main class="min-h-dvh bg-slate-950 p-4 text-slate-100 sm:p-6">
	<div class="mx-auto max-w-[1500px]">
		<header class="flex flex-wrap items-end justify-between gap-4">
			<div>
				<p class="text-sm font-medium text-cyan-300">Pack diagnostic</p>
				<h1 class="mt-1 text-3xl font-semibold">Lobby field map</h1>
				<p class="mt-2 text-sm text-slate-400">
					Every semantic anchor is projected from authored world X/Z coordinates.
				</p>
			</div>
			<div class="flex gap-2 text-sm">
				<button class:active={showLabels} class="control" onclick={() => (showLabels = !showLabels)}
					>Labels</button
				>
				<button
					class:active={stationOnly}
					class="control"
					onclick={() => (stationOnly = !stationOnly)}>Stations only</button
				>
			</div>
		</header>

		{#if error}
			<p class="mt-6 text-red-300" role="alert">{error}</p>
		{:else if !map}
			<p class="mt-6 text-slate-400">Loading pack assets and semantics…</p>
		{:else}
			<div class="mt-6 grid gap-5 xl:grid-cols-[minmax(0,1fr)_21rem]">
				<section class="overflow-hidden rounded-xl bg-slate-900 shadow-xl shadow-black/30">
					<div class="relative aspect-[1282/913]">
						<img
							class="absolute inset-0 size-full object-contain"
							src={map.imageUrl}
							alt="Top-down field with semantic point overlay"
						/>
						{#each stationAreas as area (area.name)}
							<div
								class="station-area"
								style:left={`${area.position.left}%`}
								style:top={`${area.position.top}%`}
								style:width={`${area.position.width}%`}
								style:height={`${area.position.height}%`}
								title={`${area.name} semantic area`}
							></div>
						{/each}
						{#each points as item (item.name)}
							<button
								class:station={item.station}
								class="point"
								style:left={`${item.position?.left}%`}
								style:top={`${item.position?.top}%`}
								title={`${item.name}: ${item.point.map((value) => value.toFixed(3)).join(', ')}`}
							>
								{#if showLabels}<span>{item.name}</span>{/if}
							</button>
						{/each}
					</div>
				</section>
				<aside class="rounded-xl bg-slate-900 p-4">
					<h2 class="font-semibold">Projection</h2>
					<dl class="mt-3 space-y-2 text-sm text-slate-400">
						<div>
							<dt class="text-slate-500">World X</dt>
							<dd>{map.topDown.worldBounds.min[0]} → {map.topDown.worldBounds.max[0]} m</dd>
						</div>
						<div>
							<dt class="text-slate-500">World Z</dt>
							<dd>{map.topDown.worldBounds.min[1]} → {map.topDown.worldBounds.max[1]} m</dd>
						</div>
						<div>
							<dt class="text-slate-500">Vertical direction</dt>
							<dd>{map.topDown.verticalDirection}</dd>
						</div>
						<div>
							<dt class="text-slate-500">Visible points</dt>
							<dd>{points.length}</dd>
						</div>
					</dl>
					<p class="mt-5 text-xs leading-5 text-slate-500">
						Cyan rectangles are the authored lobby footprints. Hover a point for its raw semantic
						coordinate.
					</p>
					<section class="mt-5 border-t border-slate-800 pt-5">
						<h2 class="font-semibold">Footprint tuner</h2>
						<label class="mt-3 grid gap-1 text-xs text-slate-400">
							Station
							<select
								class="rounded-md border border-slate-700 bg-slate-950 px-2 py-2 text-sm text-slate-100"
								bind:value={selectedStationId}
							>
								{#each map.stations as station (station.slotId)}
									<option value={station.slotId}>{station.label}</option>
								{/each}
							</select>
						</label>
						{#if selectedStation?.area.semanticBounds}
							<p class="mt-3 text-xs leading-5 text-slate-400">
								This footprint comes from <code>{selectedStation.area.semanticBounds}</code> in
								<code>field.semantics.json</code>. Tune the semantic mesh, not the lobby UI.
							</p>
						{:else if selectedStation}
							{@const footprint = footprintFor(selectedStation)}
							<div class="mt-3 grid grid-cols-2 gap-2">
								<label class="grid gap-1 text-xs text-slate-400">
									Width (m)
									<input
										class="rounded-md border border-slate-700 bg-slate-950 px-2 py-2 text-sm text-slate-100"
										type="number"
										min="0.1"
										step="0.02"
										value={footprint.widthM}
										oninput={(event) =>
											setFootprint(
												'widthM',
												Number((event.currentTarget as HTMLInputElement).value)
											)}
									/>
								</label>
								<label class="grid gap-1 text-xs text-slate-400">
									Height (m)
									<input
										class="rounded-md border border-slate-700 bg-slate-950 px-2 py-2 text-sm text-slate-100"
										type="number"
										min="0.1"
										step="0.02"
										value={footprint.heightM}
										oninput={(event) =>
											setFootprint(
												'heightM',
												Number((event.currentTarget as HTMLInputElement).value)
											)}
									/>
								</label>
							</div>
							<pre
								class="mt-3 overflow-x-auto rounded-md bg-slate-950 p-3 text-xs text-cyan-200">{JSON.stringify(
									{ area: footprint },
									null,
									2
								)}</pre>
						{/if}
					</section>
				</aside>
			</div>
		{/if}
	</div>
</main>

<style>
	.control {
		border-radius: 0.5rem;
		background: rgb(51 65 85);
		padding: 0.5rem 0.75rem;
		color: rgb(203 213 225);
	}
	.control:hover,
	.active {
		background: rgb(8 145 178);
		color: white;
	}
	.point {
		position: absolute;
		width: 0.65rem;
		height: 0.65rem;
		transform: translate(-50%, -50%);
		border-radius: 999px;
		background: rgb(248 113 113);
		outline: 1px solid rgb(255 255 255 / 80%);
	}
	.point span {
		position: absolute;
		left: 0.65rem;
		top: -0.3rem;
		white-space: nowrap;
		border-radius: 0.25rem;
		background: rgb(2 6 23 / 90%);
		padding: 0.15rem 0.3rem;
		color: white;
		font:
			10px ui-monospace,
			monospace;
	}
	.station {
		background: rgb(34 211 238);
		width: 0.85rem;
		height: 0.85rem;
	}
	.station-area {
		position: absolute;
		border: 1px solid rgb(34 211 238 / 90%);
		background: rgb(34 211 238 / 12%);
		pointer-events: none;
	}
</style>
