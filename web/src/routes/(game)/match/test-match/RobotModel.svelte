<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';

	let {
		player,
		physics,
		local,
		isIntaking = false,
		isOuttaking = false
	}: {
		player: MatchPlayer;
		physics: MatchPhysics;
		local: boolean;
		isIntaking?: boolean;
		isOuttaking?: boolean;
	} = $props();

	let intakeRotation = $state(0);
	let flywheelRotation = $state(0);

	useTask((delta) => {
		if (isIntaking || (player as any).intakePower > 0) {
			intakeRotation += delta * 20;
		}
		if (isOuttaking || (player as any).outtakePower > 0) {
			flywheelRotation += delta * 45;
		}
	});

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

	// Visual indicators for stored balls inside the hopper
	const visibleBalls = $derived(
		Array.from({ length: Math.min(player.storedBalls, 6) }, (_, i) => {
			const row = Math.floor(i / 2);
			const col = i % 2;
			return [
				(col - 0.5) * 0.18,
				physics.robotHeightM * 0.1 + row * 0.1,
				(row * 0.08) - 0.05
			] as [number, number, number];
		})
	);
</script>

<T.Group position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
	<T.Mesh castShadow receiveShadow>
		<T.BoxGeometry args={chassisArgs} />
		<T.MeshStandardMaterial
			color={player.color}
			emissive={local ? player.color : '#000000'}
			emissiveIntensity={local ? 0.25 : 0}
			roughness={0.4}
			metalness={0.3}
		/>
	</T.Mesh>
	<T.Mesh
		castShadow
		receiveShadow
		position={[physics.robotWidthM * 0.5 + 0.035, -physics.robotHeightM * 0.18, 0]}
	>
		<T.BoxGeometry args={wheelArgs} />
		<T.MeshStandardMaterial color="#111827" />
	</T.Mesh>
	<T.Mesh
		castShadow
		receiveShadow
		position={[-physics.robotWidthM * 0.5 - 0.035, -physics.robotHeightM * 0.18, 0]}
	>
		<T.BoxGeometry args={wheelArgs} />
		<T.MeshStandardMaterial color="#111827" />
	</T.Mesh>
	<T.Mesh castShadow receiveShadow position={[0, 0, -physics.robotLengthM * 0.5 - 0.06]}>
		<T.BoxGeometry args={headingArgs} />
		<T.MeshStandardMaterial color="#f8fafc" emissive="#38bdf8" emissiveIntensity={0.4} />
	</T.Mesh>

	{#if physics.intakeEnabled}
		<T.Group
			position={[
				0,
				physics.intakeCenterHeightM - physics.robotHeightM * 0.5,
				-physics.intakeForwardOffsetM
			]}
		>
			<T.Mesh
				castShadow
				receiveShadow
				rotation={[intakeRotation, 0, Math.PI * 0.5]}
			>
				<T.CylinderGeometry args={intakeArgs} />
				<T.MeshStandardMaterial
					color={isIntaking ? '#06b6d4' : '#22d3ee'}
					emissive={isIntaking ? '#06b6d4' : '#000000'}
					emissiveIntensity={isIntaking ? 0.8 : 0}
					roughness={0.4}
				/>
			</T.Mesh>
		</T.Group>
	{/if}

	{#if physics.outtakeHeightM > 0}
		<T.Group
			position={[
				0,
				physics.outtakeHeightM - physics.robotHeightM * 0.5,
				-physics.outtakeForwardOffsetM
			]}
		>
			<T.Mesh
				castShadow
				receiveShadow
				rotation={[flywheelRotation, 0, Math.PI * 0.5]}
			>
				<T.CylinderGeometry args={[0.06, 0.06, physics.flywheelWidthM, 12]} />
				<T.MeshStandardMaterial
					color={isOuttaking ? '#84cc16' : '#a3e635'}
					emissive={isOuttaking ? '#a3e635' : '#000000'}
					emissiveIntensity={isOuttaking ? 1.0 : 0}
					roughness={0.2}
					metalness={0.6}
				/>
			</T.Mesh>
			<T.Mesh
				castShadow
				receiveShadow
				position={[physics.flywheelWidthM * 0.5 + 0.025, 0, 0]}
				rotation={[0, 0, Math.PI * 0.5]}
			>
				<T.CylinderGeometry args={[0.035, 0.035, 0.05, 8]} />
				<T.MeshStandardMaterial color="#4d7c0f" />
			</T.Mesh>
			<T.Mesh
				castShadow
				receiveShadow
				position={[-physics.flywheelWidthM * 0.5 - 0.025, 0, 0]}
				rotation={[0, 0, Math.PI * 0.5]}
			>
				<T.CylinderGeometry args={[0.035, 0.035, 0.05, 8]} />
				<T.MeshStandardMaterial color="#4d7c0f" />
			</T.Mesh>
		</T.Group>
	{/if}

	<!-- Hopper stored balls visual -->
	{#each visibleBalls as pos}
		<T.Mesh position={pos}>
			<T.SphereGeometry args={[0.045, 12, 12]} />
			<T.MeshStandardMaterial color="#f97316" roughness={0.3} emissive="#ea580c" emissiveIntensity={0.2} />
		</T.Mesh>
	{/each}

	<HTML position={[0, physics.robotHeightM * 0.5 + 0.55, 0]} center>
		<div
			class="pointer-events-none flex flex-col items-center gap-0.5 rounded-lg border border-white/10 bg-black/80 px-2.5 py-1 backdrop-blur-md shadow-lg font-sans text-xs"
		>
			<div class="flex items-center gap-1.5 font-bold tracking-wide text-white">
				<span class="size-2 rounded-full" style="background-color: {player.color}"></span>
				<span>{player.name}</span>
				{#if local}
					<span class="rounded bg-primary/20 px-1 py-0.2 text-[10px] font-mono text-primary font-semibold">YOU</span>
				{/if}
			</div>
			<div class="flex items-center gap-1.5 text-[10px] text-gray-300 font-mono">
				<span>🏀 {player.storedBalls}/{player.capacity}</span>
				{#if isIntaking}
					<span class="text-cyan-400 font-semibold animate-pulse">[INTAKE]</span>
				{/if}
				{#if isOuttaking}
					<span class="text-lime-400 font-semibold animate-pulse">[SHOOT]</span>
				{/if}
			</div>
		</div>
	</HTML>
</T.Group>

