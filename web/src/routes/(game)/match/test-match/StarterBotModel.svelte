<script lang="ts">
	import { T, useTask, useThrelte } from '@threlte/core';
	import { useGltf, useMeshopt } from '@threlte/extras';
	import { Box3, FrontSide, LOD, Mesh, MeshStandardMaterial, Object3D, Vector3 } from 'three';

	let {
		assetUrl,
		detailAssetUrl,
		climbing = false,
		wheelAngle = 0
	}: {
		assetUrl: string;
		detailAssetUrl?: string;
		climbing?: boolean;
		wheelAngle?: number;
	} = $props();
	const meshoptDecoder = useMeshopt();
	const robotGltf = useGltf(assetUrl, { meshoptDecoder });
	const detailGltf = useGltf(detailAssetUrl ?? assetUrl, { meshoptDecoder });
	const { camera } = useThrelte();
	const bounds = new Box3();
	const center = new Vector3();
	let model = $state<Object3D | null>(null);
	let renderedWheelAngle = 0;

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
			renderedWheelAngle = 0;
			return;
		}

		const detailScene = $detailGltf?.scene;
		if (!detailScene) return;
		const lod = new LOD();
		lod.autoUpdate = false;
		lod.addLevel(prepareModel(detailScene), 0);
		lod.addLevel(prepareModel(scene), 2.2);
		model = lod;
		renderedWheelAngle = 0;
	});

	useTask((delta) => {
		if (model instanceof LOD) model.update(camera.current);
		if (!model) return;
		const targetAngle = Number.isFinite(wheelAngle)
			? wheelAngle
			: climbing
				? renderedWheelAngle + delta * 28
				: renderedWheelAngle;
		const angleDelta = targetAngle - renderedWheelAngle;
		if (Math.abs(angleDelta) < 1e-5) return;
		model.traverse((object) => {
			if (object.name.startsWith('ClimbWheel')) object.rotateX(angleDelta);
		});
		renderedWheelAngle = targetAngle;
	});
</script>

{#if model}<T is={model} />{/if}
