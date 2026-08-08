<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { Vector3, type PerspectiveCamera } from 'three';

	type TrackedPlayer = {
		x: number;
		y: number;
		z: number;
	};

	let {
		player,
		direction = 'north',
		distance = 8
	}: {
		player?: TrackedPlayer;
		direction?: 'north' | 'south';
		distance?: number;
	} = $props();

	let cameraRef: PerspectiveCamera | undefined = $state();
	const clampedDistance = $derived(Math.min(18, Math.max(1, distance)));
	const target = new Vector3();

	useTask(() => {
		const camera = cameraRef;
		if (!camera || !player) return;

		// The player transform is already interpolated by the scene. Using that
		// exact same transform here prevents camera/robot phase lag and jitter.
		target.set(player.x, Math.max(player.y, 0.2), player.z);
		const southOffset = direction === 'north' ? clampedDistance : -clampedDistance;
		camera.position.set(target.x, target.y + clampedDistance, target.z + southOffset);
		camera.up.set(0, 1, 0);
		camera.lookAt(target);
	});
</script>

<T.PerspectiveCamera
	bind:ref={cameraRef}
	makeDefault
	position={[0, clampedDistance, direction === 'north' ? clampedDistance : -clampedDistance]}
	fov={48}
	near={0.05}
	far={100}
/>
