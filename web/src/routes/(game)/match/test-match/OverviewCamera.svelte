<script lang="ts">
	import { T } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
	import type { PerspectiveCamera } from 'three';

	let { fov = 50 }: { fov?: number } = $props();
	let camera: PerspectiveCamera | undefined = $state();

	$effect(() => {
		if (!camera) return;
		camera.fov = fov;
		camera.updateProjectionMatrix();
	});
</script>

<T.PerspectiveCamera
	bind:ref={camera}
	makeDefault
	position={[11, 12, 14]}
	{fov}
	near={0.05}
	far={100}
>
	<OrbitControls target={[0, 0, 0]} enablePan={false} minDistance={8} maxDistance={28} />
</T.PerspectiveCamera>
