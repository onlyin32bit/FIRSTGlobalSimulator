<script lang="ts">
	import { T, useThrelte } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import { Quaternion, Vector3, Matrix4 } from 'three';
	import { parseRobotColliders } from './robot-collision';

	type DebugBox = {
		id: string;
		center: [number, number, number];
		size: [number, number, number];
		quaternion: [number, number, number, number];
		shape: 'box' | 'frustum';
		innerRadius?: number;
	};
	type AssimpNode = { name?: string; meshes?: number[]; transformation?: number[] };
	type AssimpScene = {
		rootnode?: { children?: AssimpNode[] };
		meshes?: Array<{ vertices?: number[]; faces?: number[][] }>;
	};

	let {
		physicsUrl,
		semanticsUrl,
		height,
		climber,
		ballContacts = [],
		ballPositions = new Float32Array(),
		robotPosition,
		robotQuaternion
	}: {
		physicsUrl?: string;
		semanticsUrl?: string;
		height: number;
		climber?: {
			wheelParts: string[];
			grooveRootRadiusM: number;
			grooveOuterRadiusM: number;
		};
		ballContacts?: string[][];
		ballPositions?: Float32Array;
		robotPosition: [number, number, number];
		robotQuaternion: [number, number, number, number];
	} = $props();

	let colliders = $state<DebugBox[]>([]);
	let semantics = $state<DebugBox[]>([]);
	let hovered = $state<DebugBox | null>(null);
	const { invalidate } = useThrelte();

	// Collider ids the server reports a ball is touching this tick. A box lit
	// red is exactly what a ball is resting on or wedged against.
	const touchedIds = $derived(new Set(ballContacts.flat()));
	const contactBalls = $derived.by(() => {
		const inverseRobot = new Quaternion(...robotQuaternion).invert();
		const correction = new Vector3(0, height * 0.5, 0);
		const balls: Array<{ index: number; position: [number, number, number]; contacts: string[] }> = [];
		for (let index = 0; index < ballContacts.length; index += 1) {
			const contacts = ballContacts[index] ?? [];
			const offset = index * 3;
			if (!contacts.length || offset + 2 >= ballPositions.length) continue;
			const local = new Vector3(
				ballPositions[offset]! - robotPosition[0],
				ballPositions[offset + 1]! - robotPosition[1],
				ballPositions[offset + 2]! - robotPosition[2]
			).applyQuaternion(inverseRobot).add(correction);
			// The authored robot assets use the same 180-degree display correction
			// as the collider group below.
			local.x *= -1;
			local.z *= -1;
			balls.push({ index, position: [local.x, local.y, local.z], contacts });
		}
		return balls;
	});

	function asBoxes(scene: AssimpScene): DebugBox[] {
		// Use the exact OBB collision parser so that the extruded 0.01m minimum half-extents
		// are accurately reflected in the debug view.
		const parsedColliders = parseRobotColliders(scene);
		const boxes = parsedColliders.map((collider) => {
			const size = collider.halfExtents.map((e) => e * 2) as [number, number, number];
			const matrix = new Matrix4().makeBasis(
				new Vector3(...collider.axes[0]),
				new Vector3(...collider.axes[1]),
				new Vector3(...collider.axes[2])
			);
			const q = new Quaternion().setFromRotationMatrix(matrix);
			return {
				id: collider.id,
				center: collider.center,
				size,
				quaternion: [q.x, q.y, q.z, q.w] as [number, number, number, number],
				shape: 'box' as const
			};
		});
		
		const wheelParts = climber?.wheelParts ?? [];
		const wheels = boxes.filter((box) => wheelParts.includes(box.id));
		if (wheels.length < 2) return boxes;
		const groove = wheels
			.reduce((sum, wheel) => sum.add(new Vector3(...wheel.center)), new Vector3())
			.multiplyScalar(1 / wheels.length);
		return boxes.map((box) => {
			if (!wheelParts.includes(box.id)) return box;
			const axleIndex = box.size.indexOf(Math.min(...box.size));
			const outerRadius = climber?.grooveOuterRadiusM ?? 0.038;
			const innerRadius = climber?.grooveRootRadiusM ?? 0.012;
			const axis = groove
				.clone()
				.sub(new Vector3(...box.center))
				.normalize();
			const orientation = new Quaternion().setFromUnitVectors(new Vector3(0, 1, 0), axis);
			return {
				...box,
				shape: 'frustum' as const,
				size: [outerRadius, box.size[axleIndex]!, outerRadius] as [number, number, number],
				innerRadius,
				quaternion: [orientation.x, orientation.y, orientation.z, orientation.w] as [
					number,
					number,
					number,
					number
				]
			};
		});
	}

	$effect(() => {
		let cancelled = false;
		const load = async () => {
			try {
				const [physics, semantic] = await Promise.all([
					physicsUrl
						? fetch(physicsUrl).then((response) => response.json() as Promise<AssimpScene>)
						: null,
					semanticsUrl
						? fetch(semanticsUrl).then((response) => response.json() as Promise<AssimpScene>)
						: null
				]);
				if (cancelled) return;
				colliders = physics ? asBoxes(physics) : [];
				semantics = semantic ? asBoxes(semantic) : [];
				invalidate();
			} catch (error) {
				console.warn('[robot-debug] unable to load authored robot debug data', error);
			}
		};
		void load();
		return () => {
			cancelled = true;
		};
	});
</script>

	<!-- Match the imported model's ground origin and 180° display correction. -->
<T.Group position={[0, -height * 0.5, 0]} rotation={[0, Math.PI, 0]}>
	{#each colliders as collider (collider.id)}
		{@const touched = touchedIds.has(collider.id)}
		<!-- Filled volume: makes thin planes and internal panels visible in 3D. -->
		<T.Mesh
			position={collider.center}
			quaternion={collider.quaternion}
			scale={collider.shape === 'box' ? collider.size : [1, 1, 1]}
			renderOrder={touched ? 31 : 28}
			frustumCulled={false}
		>
			{#if collider.shape === 'frustum'}
				<T.CylinderGeometry
					args={[collider.innerRadius ?? 0.012, collider.size[0], collider.size[1], 20]}
				/>
			{:else}
				<T.BoxGeometry args={[1, 1, 1]} />
			{/if}
			<T.MeshBasicMaterial
				color={touched ? '#ef4444' : collider.id.startsWith('ClimbWheel') ? '#facc15' : '#0284c7'}
				transparent
				opacity={touched ? 0.55 : 0.16}
				depthTest={false}
				depthWrite={false}
			/>
		</T.Mesh>
		<T.Mesh
			position={collider.center}
			quaternion={collider.quaternion}
			scale={collider.shape === 'box' ? collider.size : [1, 1, 1]}
			renderOrder={touched ? 32 : 30}
			frustumCulled={false}
		>
			{#if collider.shape === 'frustum'}
				<T.CylinderGeometry
					args={[collider.innerRadius ?? 0.012, collider.size[0], collider.size[1], 20]}
				/>
			{:else}
				<T.BoxGeometry args={[1, 1, 1]} />
			{/if}
			<T.MeshBasicMaterial
				color={touched
					? '#ef4444'
					: collider.id.startsWith('ClimbWheel')
						? '#facc15'
						: '#0ea5e9'}
			wireframe
			transparent
			opacity={touched ? 1 : collider.id.startsWith('ClimbWheel') ? 1 : 0.9}
				depthTest={false}
				depthWrite={false}
			/>
		</T.Mesh>
		{#if touched}
			<HTML position={collider.center} center>
				<div
					class="pointer-events-none rounded bg-red-600/90 px-1 py-0.5 font-mono text-[10px] font-bold text-white shadow"
				>
					{collider.id}
				</div>
			</HTML>
		{/if}
	{/each}
	{#each contactBalls as ball (ball.index)}
		<T.Mesh position={ball.position} renderOrder={40} frustumCulled={false}>
			<T.SphereGeometry args={[0.075, 12, 8]} />
			<T.MeshBasicMaterial
				color="#f43f5e"
				transparent
				opacity={0.95}
				depthTest={false}
				depthWrite={false}
			/>
		</T.Mesh>
		<HTML position={ball.position} center>
			<div class="pointer-events-none rounded bg-rose-600/95 px-1 py-0.5 font-mono text-[10px] font-bold text-white shadow">
				ball {ball.index} · {ball.contacts.join(', ')}
			</div>
		</HTML>
	{/each}
	{#each semantics as semantic (semantic.id)}
		<T.Mesh
			position={semantic.center}
			quaternion={semantic.quaternion}
			scale={semantic.size}
			renderOrder={31}
			frustumCulled={false}
		>
			<T.BoxGeometry args={[1, 1, 1]} />
			<T.MeshBasicMaterial
				color="#22d3ee"
				wireframe
				transparent
				opacity={0.9}
				depthTest={false}
				depthWrite={false}
			/>
		</T.Mesh>
		<HTML position={semantic.center} center>
			<div
				role="tooltip"
				class="cursor-help rounded bg-slate-950/90 px-1.5 py-0.5 font-mono text-[10px] text-cyan-100 shadow"
				onmouseenter={() => (hovered = semantic)}
				onmouseleave={() => (hovered = null)}
			>
				{semantic.id}
				{#if hovered?.id === semantic.id}
					<div class="mt-1 text-[9px] whitespace-nowrap text-slate-300">
						{semantic.size.map((value) => value.toFixed(3)).join(' × ')} m
					</div>
				{/if}
			</div>
		</HTML>
	{/each}
</T.Group>
