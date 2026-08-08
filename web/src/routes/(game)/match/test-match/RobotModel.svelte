<script lang="ts">
	import { T, useThrelte } from '@threlte/core';
	import { HTML, useGltf, useMeshopt } from '@threlte/extras';
	import {
		BufferAttribute,
		BufferGeometry,
		Group,
		Matrix4,
		Mesh,
		MeshBasicMaterial,
		Object3D
	} from 'three';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';

	let {
		player,
		physics,
		robotAssets,
		local,
		showBounds = false
	}: {
		player: MatchPlayer;
		physics: MatchPhysics;
		robotAssets: { visual: string; physics: string };
		local: boolean;
		showBounds?: boolean;
	} = $props();

	type RobotPhysicsNode = { name?: string; transformation?: number[]; meshes?: number[] };
	type RobotPhysicsAsset = {
		rootnode?: { children?: RobotPhysicsNode[] };
		meshes?: Array<{ vertices?: number[]; faces?: number[][] }>;
	};

	const meshoptDecoder = useMeshopt();
	const { invalidate } = useThrelte();
	const robotGltf = useGltf(robotAssets.visual, { meshoptDecoder });
	let robotPhysicsAsset = $state<RobotPhysicsAsset | null>(null);
	let physicsColliderCount = $state(0);
	let physicsColliderNames = $state<string[]>([]);
	const physicsDebugMeshes = new Group();
	physicsDebugMeshes.name = 'robot-authored-physics-colliders';
	const physicsDebugMaterial = new MeshBasicMaterial({
		color: '#ef4444',
		wireframe: true,
		transparent: true,
		opacity: 0.95,
		depthWrite: false,
		depthTest: false
	});

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
				minimum = Math.min(minimum, matrix[4] * x + matrix[5] * y + matrix[6] * z + matrix[7]);
			}
		}
		return Number.isFinite(minimum) ? -minimum : 0;
	}

	const floorOffset = $derived(authoredFloorOffset(robotPhysicsAsset));

	$effect(() => {
		const asset = robotPhysicsAsset;
		physicsDebugMeshes.traverse((object) => {
			if (object instanceof Mesh) object.geometry.dispose();
		});
		physicsDebugMeshes.clear();
		physicsColliderCount = 0;
		physicsColliderNames = [];
		if (!asset?.rootnode?.children || !asset.meshes) {
			invalidate();
			return;
		}

		for (const node of asset.rootnode.children) {
			const meshIndex = node.meshes?.[0];
			const source = Number.isInteger(meshIndex) ? asset.meshes[meshIndex!] : undefined;
			if (!source?.vertices || !source.faces?.length) continue;

			const geometry = new BufferGeometry();
			geometry.setAttribute('position', new BufferAttribute(new Float32Array(source.vertices), 3));
			const indices: number[] = [];
			for (const face of source.faces) {
				if (face.length < 3) continue;
				indices.push(face[0], face[1], face[2]);
				if (face.length === 4) indices.push(face[0], face[2], face[3]);
			}
			if (!indices.length) {
				geometry.dispose();
				continue;
			}
			geometry.setIndex(indices);
			if (node.transformation?.length === 16) {
				geometry.applyMatrix4(new Matrix4().fromArray(node.transformation).transpose());
			}
			const collider = new Mesh(geometry, physicsDebugMaterial);
			collider.name = node.name ?? `collider-${physicsColliderCount}`;
			collider.renderOrder = 30;
			physicsDebugMeshes.add(collider);
			physicsColliderCount += 1;
			physicsColliderNames = [...physicsColliderNames, collider.name];
		}
		invalidate();
	});

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
	{#if showBounds}
		<!-- Exact authored collider meshes from bot.physics.json. They share the
		     same floor-relative/orientation wrapper as the visual GLB. -->
		<T.Group
			position={[0, -physics.robotHeightM * 0.5 + floorOffset, 0]}
			rotation={[0, Math.PI, 0]}
		>
			<T is={physicsDebugMeshes} />
		</T.Group>
	{/if}
	{#await robotGltf then gltf}
		<!-- The authored robot faces +Z; the simulator's forward direction is -Z. -->
		<T.Group
			position={[0, -physics.robotHeightM * 0.5 + floorOffset, 0]}
			rotation={[0, Math.PI, 0]}
		>
			<T is={configureRobotVisual(gltf.scene)} />
		</T.Group>
	{/await}

	<HTML position={[0, physics.robotHeightM * 0.5 + 0.55, 0]} center>
		<div
			class="pointer-events-none flex flex-col items-center gap-0.5 rounded-lg border border-white/10 bg-black/80 px-2.5 py-1 font-sans text-xs shadow-lg backdrop-blur-md"
		>
			<div class="flex items-center gap-1.5 font-bold tracking-wide text-white">
				<span class="size-2 rounded-full" style="background-color: {player.color}"></span>
				<span>{player.name}</span>
				{#if local}
					<span
						class="py-0.2 rounded bg-primary/20 px-1 font-mono text-[10px] font-semibold text-primary"
						>YOU</span
					>
				{/if}
			</div>
			{#if showBounds}
				<div class="font-mono text-[10px] text-red-200">
					physics: {physicsColliderCount} colliders
				</div>
				{#if physicsColliderNames.some((name) => name.toLowerCase().includes('climb'))}
					<div class="font-mono text-[10px] text-amber-200">ClimbWheel colliders loaded</div>
				{/if}
			{/if}
		</div>
	</HTML>
</T.Group>
