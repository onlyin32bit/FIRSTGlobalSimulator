<script lang="ts">
	import { T, useThrelte } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import { Euler, Matrix4, Quaternion, Vector3 } from 'three';

	type DebugBox = {
		id: string;
		center: [number, number, number];
		size: [number, number, number];
		rotation: [number, number, number];
		quaternion?: [number, number, number, number];
		shape: 'box' | 'frustum';
		innerRadius?: number;
	};
	type AssimpNode = { name?: string; meshes?: number[]; transformation?: number[] };
	type AssimpScene = {
		rootnode?: { children?: AssimpNode[] };
		meshes?: Array<{ vertices?: number[] }>;
	};

	let {
		physicsUrl,
		semanticsUrl,
		height,
		climber
	}: {
		physicsUrl?: string;
		semanticsUrl?: string;
		height: number;
		climber?: {
			wheelParts: string[];
			grooveRootRadiusM: number;
			grooveOuterRadiusM: number;
		};
	} = $props();

	let colliders = $state<DebugBox[]>([]);
	let semantics = $state<DebugBox[]>([]);
	let hovered = $state<DebugBox | null>(null);
	const { invalidate } = useThrelte();

	function asBoxes(scene: AssimpScene): DebugBox[] {
		const boxes = (scene.rootnode?.children ?? []).flatMap((node) => {
			const mesh = Number.isInteger(node.meshes?.[0]) ? scene.meshes?.[node.meshes![0]!] : null;
			const values = mesh?.vertices;
			if (!node.name || !values?.length) return [];
			const min = [Infinity, Infinity, Infinity];
			const max = [-Infinity, -Infinity, -Infinity];
			for (let index = 0; index < values.length; index += 3) {
				for (let axis = 0; axis < 3; axis += 1) {
					min[axis] = Math.min(min[axis], values[index + axis]!);
					max[axis] = Math.max(max[axis], values[index + axis]!);
				}
			}
			const matrixValues = node.transformation;
			if (!matrixValues || matrixValues.length < 16) return [];
			const centerLocal = min.map((value, axis) => (value + max[axis]!) * 0.5) as [
				number,
				number,
				number
			];
			const halfLocal = min.map((value, axis) => (max[axis]! - value) * 0.5) as [
				number,
				number,
				number
			];
			const matrix = matrixValues;
			const center: [number, number, number] = [
				matrix[0]! * centerLocal[0] +
					matrix[1]! * centerLocal[1] +
					matrix[2]! * centerLocal[2] +
					matrix[3]!,
				matrix[4]! * centerLocal[0] +
					matrix[5]! * centerLocal[1] +
					matrix[6]! * centerLocal[2] +
					matrix[7]!,
				matrix[8]! * centerLocal[0] +
					matrix[9]! * centerLocal[1] +
					matrix[10]! * centerLocal[2] +
					matrix[11]!
			];
			const axes = [
				new Vector3(matrix[0], matrix[4], matrix[8]),
				new Vector3(matrix[1], matrix[5], matrix[9]),
				new Vector3(matrix[2], matrix[6], matrix[10])
			];
			const scale = axes.map((axis) => Math.max(axis.length(), 0.0001));
			axes.forEach((axis) => axis.normalize());
			const euler = new Euler().setFromRotationMatrix(
				new Matrix4().makeBasis(axes[0]!, axes[1]!, axes[2]!)
			);
			return [
				{
					id: node.name,
					center,
					size: halfLocal.map((extent, axis) => Math.max(0.02, extent * scale[axis]! * 2)) as [
						number,
						number,
						number
					],
					rotation: [euler.x, euler.y, euler.z] as [number, number, number],
					shape: 'box' as const
				}
			];
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
				rotation: [0, 0, 0] as [number, number, number],
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
		<T.Mesh
			position={collider.center}
			rotation={collider.rotation}
			quaternion={collider.quaternion}
			scale={collider.shape === 'box' ? collider.size : [1, 1, 1]}
			renderOrder={30}
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
				color={collider.id.startsWith('ClimbWheel') ? '#facc15' : '#c084fc'}
				wireframe
				transparent
				opacity={collider.id.startsWith('ClimbWheel') ? 0.95 : 0.5}
				depthTest={false}
				depthWrite={false}
			/>
		</T.Mesh>
	{/each}
	{#each semantics as semantic (semantic.id)}
		<T.Mesh
			position={semantic.center}
			rotation={semantic.rotation}
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
