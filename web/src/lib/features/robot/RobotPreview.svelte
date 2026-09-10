<script lang="ts">
	import { T } from '@threlte/core';
	import { useGltf, useMeshopt } from '@threlte/extras';
	import { untrack } from 'svelte';
	import { Box3, Mesh, Object3D, Vector3 } from 'three';

	let { assetUrl }: { assetUrl: string } = $props();
	const meshoptDecoder = useMeshopt();
	const robotGltf = useGltf(untrack(() => assetUrl), { meshoptDecoder });
	const configuredScenes = new WeakSet<Object3D>();
	const bounds = new Box3();
	const center = new Vector3();

	function prepareRobot(scene: Object3D) {
		if (configuredScenes.has(scene)) return scene;

		scene.updateMatrixWorld(true);
		bounds.setFromObject(scene);
		bounds.getCenter(center);
		scene.position.x -= center.x;
		scene.position.z -= center.z;
		scene.position.y -= bounds.min.y;
		scene.traverse((object) => {
			if (object instanceof Mesh) {
				object.castShadow = true;
				object.receiveShadow = true;
			}
		});
		configuredScenes.add(scene);
		return scene;
	}
</script>

<T.Group rotation={[0, Math.PI, 0]}>
	{#await $robotGltf then gltf}
		{#if gltf?.scene}
			<T is={prepareRobot(gltf.scene)} />
		{/if}
	{/await}
</T.Group>
