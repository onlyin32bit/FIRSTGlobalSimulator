<script lang="ts">
	import { T, useThrelte, useTask } from '@threlte/core';
	import { OrbitControls, Sky, Grid } from '@threlte/extras';
	import { World, RigidBody, AutoColliders, CollisionGroups } from '@threlte/rapier';
	import { Vector3 } from 'three';
	import { onMount } from 'svelte';
	import Robot from './Robot.svelte';
	import Balls from './Balls.svelte';
	import Field from '$lib/components/Field.svelte';
	import HumanPlayer from './HumanPlayer.svelte';

	import { resetScores } from '$lib/scoreStore';
	import type { ZoneAABB } from '$lib/scoreStore';
	import { matchSlotsStore, activeRobotSlotId, humanPlayerAlliance } from './stores';

	type HumanPlayerBounds = {
		minX: number;
		maxX: number;
		minZ: number;
		maxZ: number;
	};

	type MatchRole = 'robot-controller' | 'human-player';
	let {
		resetTrigger = 0,
		fov = 75,
		speed = 10,
		potatoMode = false,
		role = 'robot-controller' as MatchRole
	} = $props();
	let fieldAnchors = $state<Record<string, [number, number, number]>>({});
	let fieldZones = $state<ZoneAABB[]>([]);
	let humanPlayerPosition = $state<[number, number, number]>([-4.41658, 1.8, 2.99308]);
	let humanPlayerBounds = $state<HumanPlayerBounds>({
		minX: -5.196519,
		maxX: -3.636641,
		minZ: 2.320723,
		maxZ: 3.665437
	});
	let readyToSpawn = $state(false);

	$effect(() => {
		if (resetTrigger > 0) resetScores();
	});

	// Delay robot and ball spawning by 3 seconds so the field physics colliders
	// have time to load and settle before anything falls onto them.
	const SPAWN_DELAY_MS = 3000;

	onMount(() => {
		const timer = setTimeout(() => {
			readyToSpawn = true;
		}, SPAWN_DELAY_MS);
		return () => clearTimeout(timer);
	});

	// How high above the field surface to spawn. The field sits at roughly y≈0.
	// An extra 1.5 m of clearance ensures objects drop onto the field cleanly.
	// Balls spawn at y=1.5 (base) + up to 1.5 m of stagger so they don't all
	// land simultaneously, giving the field colliders time to receive them.
	const balls = $derived(
		Array.from({ length: 500 }).map((_, i) => {
			const angle = Math.random() * Math.PI * 2;
			const r = Math.sqrt(Math.random()) * 2.5;
			return {
				id: i,
				x: Math.cos(angle) * r,
				y: 1.5 + Math.random() * 1.5,
				z: Math.sin(angle) * r,
				color: '#f97316'
			};
		})
	);

	// --- Camera Controls ---
	const keys = { w: false, a: false, s: false, d: false, space: false, shift: false };
	let cameraTarget = $state<[number, number, number]>([0, 1, 0]);

	onMount(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			const key = e.key.toLowerCase();
			if (role === 'human-player') return;
			if (key === 'w') keys.w = true;
			if (key === 'a') keys.a = true;
			if (key === 's') keys.s = true;
			if (key === 'd') keys.d = true;
			if (key === ' ') keys.space = true;
			if (key === 'shift') keys.shift = true;
		};
		const handleKeyUp = (e: KeyboardEvent) => {
			const key = e.key.toLowerCase();
			if (role === 'human-player') return;
			if (key === 'w') keys.w = false;
			if (key === 'a') keys.a = false;
			if (key === 's') keys.s = false;
			if (key === 'd') keys.d = false;
			if (key === ' ') keys.space = false;
			if (key === 'shift') keys.shift = false;
		};
		window.addEventListener('keydown', handleKeyDown);
		window.addEventListener('keyup', handleKeyUp);
		return () => {
			window.removeEventListener('keydown', handleKeyDown);
			window.removeEventListener('keyup', handleKeyUp);
		};
	});

	const { camera } = useThrelte();

	useTask((delta) => {
		const cam = camera.current;
		if (!cam) return;

		const forward = new Vector3();
		cam.getWorldDirection(forward);
		forward.y = 0;
		if (forward.lengthSq() > 0.001) {
			forward.normalize();
		} else {
			forward.set(0, 0, -1);
		}

		const right = new Vector3().crossVectors(forward, new Vector3(0, 1, 0)).normalize();

		let moveVec = new Vector3();
		if (keys.w) moveVec.add(forward);
		if (keys.s) moveVec.sub(forward);
		if (keys.a) moveVec.sub(right);
		if (keys.d) moveVec.add(right);

		if (keys.space) moveVec.y += 1;
		if (keys.shift) moveVec.y -= 1;

		if (moveVec.lengthSq() > 0) {
			const camSpeed = speed * delta;
			moveVec.normalize().multiplyScalar(camSpeed);

			cameraTarget = [
				cameraTarget[0] + moveVec.x,
				cameraTarget[1] + moveVec.y,
				cameraTarget[2] + moveVec.z
			];
			cam.position.add(moveVec);
		}
	});
</script>

{#if role === 'human-player'}
	<HumanPlayer position={humanPlayerPosition} bounds={humanPlayerBounds} {fov} />
{:else}
	<T.PerspectiveCamera makeDefault {fov} position={[0, 5, 10]}>
		<OrbitControls target={cameraTarget} enableDamping={false} />
	</T.PerspectiveCamera>
{/if}

<!-- Environment -->
<Sky elevation={2} />
<T.AmbientLight intensity={0.5} />
<T.DirectionalLight
	position={[10, 10, 5]}
	intensity={1.5}
	castShadow
	shadow.mapSize={[potatoMode ? 512 : 1024, potatoMode ? 512 : 1024]}
/>

<Grid
	position={[0, 0.01, 0]}
	cellColor="#ffffff"
	sectionColor="#ffffff"
	sectionThickness={0}
	fadeDistance={20}
	cellSize={2}
/>

<World framerate={potatoMode ? 30 : 60}>
	{#if readyToSpawn}
		{#each $matchSlotsStore.filter((s) => s.controller !== 'disabled') as slot (slot.id)}
			{@const anchor =
				fieldAnchors[slot.spawnAnchor] ??
				(slot.alliance === 'blue' ? [3.25, 0, 0.7] : [-3.25, 0, 0.7])}
			{@const spawnPos: [number, number, number] = [anchor[0], 1.5, anchor[2]]}
			<Robot
				{resetTrigger}
				{spawnPos}
				slotId={slot.id}
				slotName={slot.name}
				alliance={slot.alliance}
				isAi={slot.controller === 'ai-bot'}
				controllerEnabled={role === 'robot-controller' && slot.id === $activeRobotSlotId}
			/>
		{/each}

		<!-- PU Foam Balls -->
		<Balls ballsData={balls} {potatoMode} {resetTrigger} scoringZones={fieldZones} />
	{/if}
	<Field
		bind:anchors={fieldAnchors}
		bind:zones={fieldZones}
		humanPlayerAlliance={$humanPlayerAlliance}
		bind:humanPlayerPosition
		bind:humanPlayerBounds
	/>

	<!-- EVA Foam Ground (FTC Tiles) -->
	<CollisionGroups groups={[0]}>
		<RigidBody type="fixed">
			<AutoColliders shape="cuboid" friction={1.2} restitution={0.5}>
				<T.Mesh position={[0, -0.5, 0]} receiveShadow>
					<T.BoxGeometry args={[40, 1, 40]} />

					<!-- High-contrast grid for FTC floor visualization (EVA foam tiles) -->
					{#if potatoMode}
						<T.MeshLambertMaterial color="#333333" />
					{:else}
						<T.MeshStandardMaterial color="#333333" roughness={0.9} metalness={0.1} />
					{/if}
				</T.Mesh>
			</AutoColliders>
		</RigidBody>
	</CollisionGroups>

	<!-- 7m x 7m Perimeter Walls (Polycarbonate style, 20cm tall) -->
	<CollisionGroups groups={[1]}>
		<RigidBody type="fixed">
			<!-- North Wall -->
			<AutoColliders shape="cuboid">
				<T.Mesh position={[0, 0.1, -3.5]} castShadow receiveShadow>
					<T.BoxGeometry args={[7.1, 0.2, 0.1]} />
					{#if potatoMode}
						<T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
					{:else}
						<T.MeshStandardMaterial
							color="#ffffff"
							roughness={0.1}
							metalness={0.1}
							transparent
							opacity={0.4}
						/>
					{/if}
				</T.Mesh>
			</AutoColliders>

			<!-- South Wall -->
			<AutoColliders shape="cuboid">
				<T.Mesh position={[0, 0.1, 3.5]} castShadow receiveShadow>
					<T.BoxGeometry args={[7.1, 0.2, 0.1]} />
					{#if potatoMode}
						<T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
					{:else}
						<T.MeshStandardMaterial
							color="#ffffff"
							roughness={0.1}
							metalness={0.1}
							transparent
							opacity={0.4}
						/>
					{/if}
				</T.Mesh>
			</AutoColliders>

			<!-- East Wall -->
			<AutoColliders shape="cuboid">
				<T.Mesh position={[3.5, 0.1, 0]} castShadow receiveShadow>
					<T.BoxGeometry args={[0.1, 0.2, 7.1]} />
					{#if potatoMode}
						<T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
					{:else}
						<T.MeshStandardMaterial
							color="#ffffff"
							roughness={0.1}
							metalness={0.1}
							transparent
							opacity={0.4}
						/>
					{/if}
				</T.Mesh>
			</AutoColliders>

			<!-- West Wall -->
			<AutoColliders shape="cuboid">
				<T.Mesh position={[-3.5, 0.1, 0]} castShadow receiveShadow>
					<T.BoxGeometry args={[0.1, 0.2, 7.1]} />
					{#if potatoMode}
						<T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
					{:else}
						<T.MeshStandardMaterial
							color="#ffffff"
							roughness={0.1}
							metalness={0.1}
							transparent
							opacity={0.4}
						/>
					{/if}
				</T.Mesh>
			</AutoColliders>
		</RigidBody>
	</CollisionGroups>
</World>
