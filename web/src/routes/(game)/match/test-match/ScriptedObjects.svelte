<script lang="ts">
	import { T, useThrelte } from '@threlte/core';
	import { DynamicDrawUsage, type InstancedMesh } from 'three';

	type ObjectFrame = {
		positions: Float32Array;
		radius: number;
		color: string;
	};

	let { frame }: { frame: ObjectFrame } = $props();
	let meshRef: InstancedMesh | undefined = $state();
	const MAX_INSTANCES = 1024;
	const BASE_RADIUS = 0.05;
	const MESH_ARGS: [undefined, undefined, number] = [undefined, undefined, MAX_INSTANCES];
	const SPHERE_ARGS: [number, number, number] = [BASE_RADIUS, 8, 6];
	const { invalidate } = useThrelte();

	$effect(() => {
		const mesh = meshRef;
		const snapshot = frame;
		if (!mesh) return;

		mesh.instanceMatrix.setUsage(DynamicDrawUsage);
		const matrices = mesh.instanceMatrix.array as Float32Array;
		const count = Math.min(snapshot.positions.length / 3, MAX_INSTANCES);
		const scale = snapshot.radius / BASE_RADIUS;

		for (let index = 0; index < count; index += 1) {
			const positionOffset = index * 3;
			const offset = index * 16;

			// Spheres only need uniform scale and translation. Writing directly to
			// the instance buffer avoids 500 Object3D updates and matrix copies.
			matrices[offset] = scale;
			matrices[offset + 1] = 0;
			matrices[offset + 2] = 0;
			matrices[offset + 3] = 0;
			matrices[offset + 4] = 0;
			matrices[offset + 5] = scale;
			matrices[offset + 6] = 0;
			matrices[offset + 7] = 0;
			matrices[offset + 8] = 0;
			matrices[offset + 9] = 0;
			matrices[offset + 10] = scale;
			matrices[offset + 11] = 0;
			matrices[offset + 12] = snapshot.positions[positionOffset];
			matrices[offset + 13] = snapshot.positions[positionOffset + 1];
			matrices[offset + 14] = snapshot.positions[positionOffset + 2];
			matrices[offset + 15] = 1;
		}

		mesh.count = count;
		mesh.instanceMatrix.needsUpdate = true;
		invalidate();
	});
</script>

<T.InstancedMesh bind:ref={meshRef} args={MESH_ARGS} frustumCulled={false}>
	<T.SphereGeometry args={SPHERE_ARGS} />
	<T.MeshBasicMaterial color={frame.color} />
</T.InstancedMesh>
