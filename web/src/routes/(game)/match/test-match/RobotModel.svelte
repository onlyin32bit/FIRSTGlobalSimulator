<script lang="ts">
	import { T } from '@threlte/core';
	import { HTML, useGltf, useMeshopt } from '@threlte/extras';
	import { Mesh, Object3D } from 'three';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';

	let {
		player,
		physics,
		robotAssets,
		local,
		isIntaking = false,
		isOuttaking = false
	}: {
		player: MatchPlayer;
		physics: MatchPhysics;
		robotAssets: { visual: string; physics: string };
		local: boolean;
		isIntaking?: boolean;
		isOuttaking?: boolean;
	} = $props();

	type RobotPhysicsNode = { transformation?: number[]; meshes?: number[] };
	type RobotPhysicsAsset = {
		rootnode?: { children?: RobotPhysicsNode[] };
		meshes?: Array<{ vertices?: number[] }>;
	};

	const meshoptDecoder = useMeshopt();
	const robotGltf = useGltf(robotAssets.visual, { meshoptDecoder });
	let robotPhysicsAsset = $state<RobotPhysicsAsset | null>(null);

	$effect(() => {
		const controller = new AbortController();
		fetch(robotAssets.physics, { signal: controller.signal })
			.then((response) => {
				if (!response.ok) throw new Error(`Unable to load ${robotAssets.physics}`);
				return response.json() as Promise<RobotPhysicsAsset>;
			})
			.then((asset) => (robotPhysicsAsset = asset))
			.catch(() => {
				if (!controller.signal.aborted) robotPhysicsAsset = null;
			});
		return () => controller.abort();
	});

	// bot.physics.json is authored in floor-relative robot coordinates. Use its
	// lowest collider vertex to keep the detailed GLB seated on the same floor
	// as the server's center-of-mass pose.
	function authoredFloorOffset(asset: RobotPhysicsAsset | null): number {
		if (!asset?.rootnode?.children || !asset.meshes) return 0;
		let minimum = Infinity;
		for (const node of asset.rootnode.children) {
			const matrix = node.transformation;
			const meshIndex = node.meshes?.[0];
			const mesh = Number.isInteger(meshIndex) ? asset.meshes[meshIndex!] : undefined;
			if (!matrix || matrix.length < 16 || !mesh?.vertices) continue;
			for (let index = 0; index + 2 < mesh.vertices.length; index += 3) {
				const x = mesh.vertices[index];
				const y = mesh.vertices[index + 1];
				const z = mesh.vertices[index + 2];
				minimum = Math.min(
					minimum,
					matrix[4] * x + matrix[5] * y + matrix[6] * z + matrix[7]
				);
			}
		}
		return Number.isFinite(minimum) ? -minimum : 0;
	}

	const floorOffset = $derived(authoredFloorOffset(robotPhysicsAsset));
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

	function configureRobotVisual(scene: Object3D): Object3D {
		const clone = scene.clone(true);
		clone.traverse((object) => {
			if (object instanceof Mesh) {
				object.castShadow = true;
				object.receiveShadow = true;
			}
		});
		return clone;
	}
</script>

<T.Group position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
	{#await robotGltf then gltf}
		<!-- The authored robot faces +Z; the simulator's forward direction is -Z. -->
		<T.Group position={[0, -physics.robotHeightM * 0.5 + floorOffset, 0]} rotation={[0, Math.PI, 0]}>
			<T is={configureRobotVisual(gltf.scene)} />
		</T.Group>
	{/await}

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
