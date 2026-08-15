<script lang="ts">
	import { T, useTask, useThrelte } from '@threlte/core';
	import { useGltf, useMeshopt } from '@threlte/extras';
	import { Box3, FrontSide, LOD, Mesh, MeshStandardMaterial, Object3D, Vector3 } from 'three';

	let { assetUrl, detailAssetUrl }: { assetUrl: string; detailAssetUrl?: string } = $props();
	const meshoptDecoder = useMeshopt();
	const robotGltf = useGltf(assetUrl, { meshoptDecoder });
	const detailGltf = useGltf(detailAssetUrl ?? assetUrl, { meshoptDecoder });
	const { camera } = useThrelte();
	const bounds = new Box3();
	const center = new Vector3();
	let model = $state<Object3D | null>(null);

	function prepareModel(scene: Object3D) {
		const instance = scene.clone(true);
		instance.updateMatrixWorld(true);
		bounds.setFromObject(instance);
		bounds.getCenter(center);
		instance.position.set(-center.x, -bounds.min.y, -center.z);
		instance.traverse((object) => {
			if (!(object instanceof Mesh)) return;
			object.castShadow = false;
			object.receiveShadow = false;
			const materials = Array.isArray(object.material) ? object.material : [object.material];
			for (const material of materials) {
				if (material instanceof MeshStandardMaterial && material.side !== FrontSide) {
					material.side = FrontSide;
					material.needsUpdate = true;
				}
			}
		});
		return instance;
	}

	$effect(() => {
		const scene = $robotGltf?.scene;
		if (!scene) return;

		if (!detailAssetUrl) {
			model = prepareModel(scene);
			return;
		}

		const detailScene = $detailGltf?.scene;
		if (!detailScene) return;
		const lod = new LOD();
		lod.autoUpdate = false;
		lod.addLevel(prepareModel(detailScene), 0);
		lod.addLevel(prepareModel(scene), 2.2);
		model = lod;
	});

	useTask(() => {
		if (model instanceof LOD) model.update(camera.current);
	});
</script>

{#if model}<T is={model} />{/if}
