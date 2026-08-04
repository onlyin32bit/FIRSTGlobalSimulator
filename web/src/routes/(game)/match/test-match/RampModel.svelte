<script lang="ts">
	import { T } from '@threlte/core';
	import type { MatchPhysics } from './match-protocol';

	let { physics }: { physics: MatchPhysics } = $props();
	const thickness = 0.05;
	const args = $derived<[number, number, number]>([
		physics.rampWidthM,
		thickness,
		physics.rampLengthM
	]);
	const angle = $derived((physics.rampAngleDeg * Math.PI) / 180);
	const position = $derived<[number, number, number]>([
		physics.rampCenterX,
		Math.sin(angle) * physics.rampLengthM * 0.5 - thickness * 0.5,
		physics.rampStartZ + Math.cos(angle) * physics.rampLengthM * 0.5
	]);
</script>

{#if physics.rampEnabled}
	<T.Mesh {position} rotation={[-angle, 0, 0]}>
		<T.BoxGeometry {args} />
		<T.MeshStandardMaterial color="#64748b" roughness={0.65} metalness={0.05} />
	</T.Mesh>
{/if}
