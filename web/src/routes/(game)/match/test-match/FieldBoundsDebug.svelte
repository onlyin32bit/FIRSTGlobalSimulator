<script lang="ts">
	import { T } from '@threlte/core';

	type Bounds = { min: [number, number, number]; max: [number, number, number] };
	type NamedBounds = Bounds & { id: string };

	let {
		colliders,
		triggers,
		boundary,
		activeTriggerIds = new Set<string>()
	}: {
		colliders: NamedBounds[];
		triggers: NamedBounds[];
		boundary: Bounds;
		activeTriggerIds?: Set<string>;
	} = $props();

	function center(bounds: Bounds): [number, number, number] {
		return [
			(bounds.min[0] + bounds.max[0]) * 0.5,
			(bounds.min[1] + bounds.max[1]) * 0.5,
			(bounds.min[2] + bounds.max[2]) * 0.5
		];
	}

	function size(bounds: Bounds): [number, number, number] {
		return [
			Math.max(0.01, bounds.max[0] - bounds.min[0]),
			Math.max(0.01, bounds.max[1] - bounds.min[1]),
			Math.max(0.01, bounds.max[2] - bounds.min[2])
		];
	}
</script>

<!-- These are the server-loaded AABBs, not browser physics bodies. -->
<T.Group>
	<!-- Green = the authoritative playable perimeter, derived from guard rail. -->
	<T.Mesh position={center(boundary)} scale={size(boundary)} renderOrder={19}>
		<T.BoxGeometry args={[1, 1, 1]} />
		<T.MeshBasicMaterial color="#22c55e" wireframe transparent opacity={0.9} depthWrite={false} />
	</T.Mesh>
	{#each colliders as collider (collider.id)}
		<T.Mesh position={center(collider)} scale={size(collider)} renderOrder={20}>
			<T.BoxGeometry args={[1, 1, 1]} />
			<T.MeshBasicMaterial color="#facc15" wireframe transparent opacity={0.72} depthWrite={false} />
		</T.Mesh>
	{/each}
	{#each triggers as trigger (trigger.id)}
		{@const active = activeTriggerIds.has(trigger.id)}
		<T.Mesh position={center(trigger)} scale={size(trigger)} renderOrder={21}>
			<T.BoxGeometry args={[1, 1, 1]} />
			<T.MeshBasicMaterial
				color={active ? '#22c55e' : '#38bdf8'}
				wireframe
				transparent
				opacity={active ? 1 : 0.48}
				depthWrite={false}
			/>
		</T.Mesh>
	{/each}
</T.Group>
