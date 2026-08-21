<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';
	import StarterBotModel, { type RobotRollerSettings } from './StarterBotModel.svelte';
	import RobotDebug from './RobotDebug.svelte';

	let {
		player,
		physics,
		local,
		visualAsset,
		detailVisualAsset,
		isIntaking = false,
		isOuttaking = false,
		rollerSettings,
		debug = false,
		physicsAsset,
		semanticsAsset,
		climber,
		isClimbing = false,
		ballContacts = [],
		ballPositions = new Float32Array()
	}: {
		player: MatchPlayer;
		physics: MatchPhysics;
		local: boolean;
		visualAsset?: string;
		detailVisualAsset?: string;
		isIntaking?: boolean;
		isOuttaking?: boolean;
		rollerSettings?: RobotRollerSettings;
		debug?: boolean;
		physicsAsset?: string;
		semanticsAsset?: string;
		climber?: {
			wheelParts: string[];
			grooveRootRadiusM: number;
			grooveOuterRadiusM: number;
		};
		isClimbing?: boolean;
		ballContacts?: string[][];
		ballPositions?: Float32Array;
	} = $props();

	let intakeRotation = $state(0);
	let flywheelRotation = $state(0);

	useTask((delta) => {
		if (isIntaking) {
			intakeRotation += delta * 20;
		}
		if (isOuttaking) {
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
			return [(col - 0.5) * 0.18, physics.robotHeightM * 0.1 + row * 0.1, row * 0.08 - 0.05] as [
				number,
				number,
				number
			];
		})
	);
</script>

<T.Group
	position={[player.x, player.y, player.z]}
	quaternion={[player.rotationX, player.rotationY, player.rotationZ, player.rotationW]}
>
	{#if visualAsset}
		<!-- Player poses are chassis-centred; the imported model is ground-authored. -->
		<T.Group position={[0, -physics.robotHeightM * 0.5, 0]} rotation={[0, Math.PI, 0]}>
			<StarterBotModel
				assetUrl={visualAsset}
				detailAssetUrl={detailVisualAsset}
				climbing={isClimbing}
				wheelAngle={player.climbWheelAngle}
				{isIntaking}
				{isOuttaking}
				{rollerSettings}
			/>
		</T.Group>
	{:else}
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
				<T.Mesh castShadow receiveShadow rotation={[intakeRotation, 0, Math.PI * 0.5]}>
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
				<T.Mesh castShadow receiveShadow rotation={[flywheelRotation, 0, Math.PI * 0.5]}>
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
		{#each visibleBalls as pos, i (i)}
			<T.Mesh position={pos}>
				<T.SphereGeometry args={[0.045, 12, 12]} />
				<T.MeshStandardMaterial
					color="#f97316"
					roughness={0.3}
					emissive="#ea580c"
					emissiveIntensity={0.2}
				/>
			</T.Mesh>
		{/each}
	{/if}
	{#if debug}
		<RobotDebug
			physicsUrl={physicsAsset}
			semanticsUrl={semanticsAsset}
			{climber}
			{ballContacts}
			ballPositions={ballPositions}
			robotPosition={[player.x, player.y, player.z]}
			robotQuaternion={[player.rotationX, player.rotationY, player.rotationZ, player.rotationW]}
			height={physics.robotHeightM}
		/>
	{/if}

	<HTML position={[0, physics.robotHeightM * 0.5 + 0.45, 0]} center>
		<div
			class="pointer-events-none font-sans text-xs font-semibold text-white drop-shadow-[0_1px_3px_rgba(0,0,0,0.9)]"
		>
			{player.name}
		</div>
	</HTML>
</T.Group>
