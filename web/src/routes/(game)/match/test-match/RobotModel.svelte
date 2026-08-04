<script lang="ts">
	import { T } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';

	let { player, physics, local }: { player: MatchPlayer; physics: MatchPhysics; local: boolean } =
		$props();
	const chassisArgs = $derived<[number, number, number]>([
		physics.robotWidthM,
		physics.robotHeightM,
		physics.robotLengthM
	]);
	const wheelArgs = $derived<[number, number, number]>([
		0.07,
		physics.robotHeightM * 0.48,
		physics.robotLengthM * 0.7
	]);
	const headingArgs: [number, number, number] = [0.2, 0.2, 0.22];
	const intakeArgs = $derived<[number, number, number, number]>([
		physics.intakeRadiusM,
		physics.intakeRadiusM,
		physics.intakeWidthM,
		12
	]);
</script>

<T.Group position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
	<T.Mesh>
		<T.BoxGeometry args={chassisArgs} />
		<T.MeshStandardMaterial
			color={player.color}
			emissive={local ? player.color : '#000000'}
			emissiveIntensity={local ? 0.2 : 0}
		/>
	</T.Mesh>
	<T.Mesh position={[physics.robotWidthM * 0.5 + 0.035, -physics.robotHeightM * 0.18, 0]}>
		<T.BoxGeometry args={wheelArgs} />
		<T.MeshStandardMaterial color="#111827" />
	</T.Mesh>
	<T.Mesh position={[-physics.robotWidthM * 0.5 - 0.035, -physics.robotHeightM * 0.18, 0]}>
		<T.BoxGeometry args={wheelArgs} />
		<T.MeshStandardMaterial color="#111827" />
	</T.Mesh>
	<T.Mesh position={[0, 0, -physics.robotLengthM * 0.5 - 0.06]}>
		<T.BoxGeometry args={headingArgs} />
		<T.MeshStandardMaterial color="#f8fafc" />
	</T.Mesh>
	{#if physics.intakeEnabled}
		<T.Mesh
			position={[
				0,
				physics.intakeCenterHeightM - physics.robotHeightM * 0.5,
				-physics.intakeForwardOffsetM
			]}
			rotation={[0, 0, Math.PI * 0.5]}
		>
			<T.CylinderGeometry args={intakeArgs} />
			<T.MeshStandardMaterial color="#22d3ee" roughness={0.7} />
		</T.Mesh>
	{/if}
	<HTML position={[0, physics.robotHeightM * 0.5 + 0.55, 0]} center>
		<div
			class="pointer-events-none rounded bg-black/75 px-2 py-1 text-xs font-semibold whitespace-nowrap text-white shadow"
		>
			{player.name}
		</div>
	</HTML>
</T.Group>
