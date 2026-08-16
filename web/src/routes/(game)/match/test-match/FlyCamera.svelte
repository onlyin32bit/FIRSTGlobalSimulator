<script lang="ts">
	import { onMount } from 'svelte';
	import { T, useTask, useThrelte } from '@threlte/core';
	import { Vector3, type PerspectiveCamera } from 'three';

	let {
		fov = 65,
		speed = 4,
		locked = false
	}: { fov?: number; speed?: number; locked?: boolean } = $props();
	let camera: PerspectiveCamera | undefined = $state();
	const { canvas, invalidate } = useThrelte();
	const pressed = new Set<string>();
	const forward = new Vector3();
	const right = new Vector3();
	const movement = new Vector3();
	let yaw = -0.72;
	let pitch = -0.35;

	$effect(() => {
		if (!camera) return;
		camera.fov = fov;
		camera.updateProjectionMatrix();
		invalidate();
	});

	$effect(() => {
		if (!locked) return;
		pressed.clear();
		if (document.pointerLockElement === canvas) document.exitPointerLock();
	});

	onMount(() => {
		const requestPointerLock = (event: PointerEvent) => {
			if (!locked && event.button === 0 && document.pointerLockElement !== canvas) {
				void canvas.requestPointerLock();
			}
		};
		const updateLook = (event: MouseEvent) => {
			if (locked || document.pointerLockElement !== canvas) return;
			yaw -= event.movementX * 0.0022;
			pitch = Math.max(-Math.PI * 0.49, Math.min(Math.PI * 0.49, pitch - event.movementY * 0.0022));
			invalidate();
		};
		const keydown = (event: KeyboardEvent) => {
			if (!locked) pressed.add(event.code);
		};
		const keyup = (event: KeyboardEvent) => pressed.delete(event.code);
		const clearKeys = () => pressed.clear();

		canvas.addEventListener('pointerdown', requestPointerLock);
		window.addEventListener('mousemove', updateLook);
		window.addEventListener('keydown', keydown);
		window.addEventListener('keyup', keyup);
		window.addEventListener('blur', clearKeys);
		return () => {
			canvas.removeEventListener('pointerdown', requestPointerLock);
			window.removeEventListener('mousemove', updateLook);
			window.removeEventListener('keydown', keydown);
			window.removeEventListener('keyup', keyup);
			window.removeEventListener('blur', clearKeys);
			if (document.pointerLockElement === canvas) document.exitPointerLock();
		};
	});

	useTask((delta) => {
		if (!camera) return;
		camera.rotation.order = 'YXZ';
		camera.rotation.set(pitch, yaw, 0);
		if (locked) return;
		movement.set(0, 0, 0);
		forward.set(0, 0, -1).applyEuler(camera.rotation);
		right.set(1, 0, 0).applyEuler(camera.rotation);
		if (pressed.has('KeyW')) movement.add(forward);
		if (pressed.has('KeyS')) movement.sub(forward);
		if (pressed.has('KeyD')) movement.add(right);
		if (pressed.has('KeyA')) movement.sub(right);
		if (pressed.has('KeyE')) movement.y += 1;
		if (pressed.has('KeyQ')) movement.y -= 1;
		if (movement.lengthSq() === 0) return;
		const boost = pressed.has('ShiftLeft') || pressed.has('ShiftRight') ? 2.5 : 1;
		camera.position.addScaledVector(movement.normalize(), speed * boost * Math.min(delta, 0.05));
	});
</script>

<T.PerspectiveCamera
	bind:ref={camera}
	makeDefault
	position={[9, 7, 11]}
	rotation={[pitch, yaw, 0]}
	{fov}
	near={0.05}
	far={150}
/>
