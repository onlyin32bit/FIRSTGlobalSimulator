<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { useGltf, useMeshopt } from '@threlte/extras';
	import { untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import {
		BatchedMesh,
		BufferGeometry,
		FrontSide,
		Matrix4,
		Mesh,
		MeshStandardMaterial,
		Object3D,
		SkinnedMesh
	} from 'three';

	import defaultRollerSettings from './robot-rollers.json';

	export type RollerConfig = {
		parts: string[];
		axis?: 'x' | 'y' | 'z' | 'X' | 'Y' | 'Z';
		speed?: number;
		direction?: number;
	};

	export type RobotRollerSettings = {
		intake?: RollerConfig;
		outtake?: RollerConfig;
		transfer?: RollerConfig;
		climb?: RollerConfig;
	};

	let {
		assetUrl,
		climbing = false,
		wheelAngle = 0,
		isIntaking = false,
		isOuttaking = false,
		rollerSettings = defaultRollerSettings as RobotRollerSettings
	}: {
		assetUrl: string;
		climbing?: boolean;
		wheelAngle?: number;
		isIntaking?: boolean;
		isOuttaking?: boolean;
		rollerSettings?: RobotRollerSettings;
	} = $props();

	const meshoptDecoder = useMeshopt();
	const cacheBuster = import.meta.env.DEV ? `?r=${Date.now()}` : '';
	const robotGltf = useGltf(`${untrack(() => assetUrl)}${cacheBuster}`, { meshoptDecoder });
	let model = $state<Object3D | null>(null);
	let renderedWheelAngle = 0;

	// ── Cached node lists resolved once after model load ──────────────────────
	type NodeCache = { nodes: Object3D[]; config: RollerConfig };
	let cachedRollers: Record<string, NodeCache> = {};

	function sanitizeName(name: string): string {
		return (name || '').toLowerCase().replace(/[^a-z0-9]/g, '');
	}

	function resolveNodes(root: Object3D, config: RollerConfig): Object3D[] {
		const targetKeys = config.parts.map((p) => sanitizeName(p));
		const found: Object3D[] = [];
		root.traverse((object) => {
			if (!object.name) return;
			const objKey = sanitizeName(object.name);
			if (targetKeys.some((k) => objKey === k || objKey.startsWith(k) || objKey.includes(k))) {
				found.push(object);
			}
		});
		return found;
	}

	function isMechanismPart(object: Object3D, root: Object3D, targetKeys: string[]): boolean {
		for (
			let current: Object3D | null = object;
			current && current !== root;
			current = current.parent
		) {
			const key = sanitizeName(current.name);
			if (
				key &&
				targetKeys.some(
					(target) => key === target || key.startsWith(target) || key.includes(target)
				)
			) {
				return true;
			}
		}
		return false;
	}

	/**
	 * CAD exports contain hundreds of immutable meshes. Rendering every mesh as a
	 * separate draw call is much more expensive than drawing them as a batch.
	 * BatchedMesh stores each unique geometry once and reuses it for repeated CAD
	 * parts, avoiding the large GPU-memory duplication caused by merging every
	 * transformed instance. Actuated mechanism subtrees remain independent.
	 */
	function batchStaticRobotMeshes(root: Object3D) {
		const rootInverse = new Matrix4();
		const relativeMatrix = new Matrix4();
		const targetKeys = Object.values(rollerSettings)
			.flatMap((config) => config?.parts ?? [])
			.map(sanitizeName);
		const batches = new SvelteMap<
			string,
			{
				material: MeshStandardMaterial;
				entries: Array<{ source: Mesh; geometry: BufferGeometry; matrix: Matrix4 }>;
			}
		>();
		root.updateMatrixWorld(true);
		rootInverse.copy(root.matrixWorld).invert();

		root.traverse((object) => {
			if (
				!(object instanceof Mesh) ||
				object instanceof SkinnedMesh ||
				object instanceof BatchedMesh ||
				isMechanismPart(object, root, targetKeys)
			)
				return;
			if (Array.isArray(object.material) || !(object.material instanceof MeshStandardMaterial))
				return;
			if (object.material.transparent || Object.keys(object.geometry.morphAttributes).length > 0)
				return;
			const attributes = Object.keys(object.geometry.attributes).sort().join(',');
			const key = `${object.material.uuid}:${attributes}:${object.geometry.index ? 'indexed' : 'plain'}`;
			const batch = batches.get(key) ?? {
				material: object.material,
				entries: []
			};
			relativeMatrix.multiplyMatrices(rootInverse, object.matrixWorld);
			// BatchedMesh does not support mirrored instance transforms. Leave those
			// uncommon CAD nodes as ordinary meshes rather than corrupting them.
			if (relativeMatrix.determinant() < 0) return;
			batch.entries.push({
				source: object,
				geometry: object.geometry,
				matrix: relativeMatrix.clone()
			});
			batches.set(key, batch);
		});

		const batchedRoot = new Object3D();
		batchedRoot.name = 'StaticRobotBatches';
		for (const { material, entries } of batches.values()) {
			if (entries.length < 2) continue;
			const geometries = new SvelteMap<string, BufferGeometry>();
			for (const { geometry } of entries) geometries.set(geometry.uuid, geometry);
			let vertexCapacity = 0;
			let indexCapacity = 0;
			for (const geometry of geometries.values()) {
				vertexCapacity += geometry.getAttribute('position').count;
				indexCapacity += geometry.index?.count ?? 0;
			}
			const mesh = new BatchedMesh(entries.length, vertexCapacity, indexCapacity, material);
			mesh.name = 'StaticRobotBatch';
			mesh.perObjectFrustumCulled = false;
			mesh.sortObjects = false;
			mesh.castShadow = false;
			mesh.receiveShadow = false;
			const geometryIds = new SvelteMap<string, number>();
			for (const geometry of geometries.values()) {
				geometryIds.set(geometry.uuid, mesh.addGeometry(geometry));
			}
			for (const { source, geometry, matrix } of entries) {
				const geometryId = geometryIds.get(geometry.uuid);
				if (geometryId === undefined) continue;
				mesh.setMatrixAt(mesh.addInstance(geometryId), matrix);
				source.visible = false;
			}
			mesh.computeBoundingBox();
			mesh.computeBoundingSphere();
			batchedRoot.add(mesh);
		}
		if (batchedRoot.children.length > 0) root.add(batchedRoot);
	}

	function buildCache(root: Object3D) {
		cachedRollers = {};
		const mechs: [string, RollerConfig | undefined][] = [
			['intake', rollerSettings.intake],
			['outtake', rollerSettings.outtake],
			['transfer', rollerSettings.transfer],
			['climb', rollerSettings.climb]
		];

		// Collect all node names for diagnostics
		const allNames: string[] = [];
		root.traverse((o) => {
			if (o.name) allNames.push(o.name);
		});
		const rollerish = allNames.filter((n) => /roller|flap/i.test(n));
		console.log('[RobotModel:v2] GLB loaded — node count:', allNames.length);
		console.log('[RobotModel:v2] roller/flap-like node names:', rollerish);
		console.log('[RobotModel:v2] rollerSettings:', JSON.stringify(rollerSettings));

		for (const [mechName, config] of mechs) {
			if (!config) {
				console.warn(`[RobotModel:v2] No config for mechanism: "${mechName}"`);
				continue;
			}
			const nodes = resolveNodes(root, config);
			cachedRollers[mechName] = { nodes, config };
			if (nodes.length === 0) {
				console.warn(
					`[RobotModel:v2] ⚠ "${mechName}" — no nodes found for parts: ${JSON.stringify(config.parts)}. ` +
						(rollerish.length === 0
							? `The loaded GLB contains NO roller/flap nodes at all — the browser served a stale cached copy or the wrong asset.`
							: `GLB has roller/flap nodes (${JSON.stringify(rollerish)}) but none matched this config.`)
				);
			} else {
				console.log(
					`[RobotModel:v2] ✓ "${mechName}" matched ${nodes.length} node(s):`,
					nodes.map((n) => n.name)
				);
			}
		}
	}

	function rotateCachedNodes(mechName: string, deltaAngle: number) {
		const entry = cachedRollers[mechName];
		if (!entry || entry.nodes.length === 0) return;
		const axis = (entry.config.axis || 'x').toLowerCase();
		const dir = entry.config.direction ?? 1;
		const angle = deltaAngle * dir;
		for (const object of entry.nodes) {
			if (axis === 'y') object.rotateY(angle);
			else if (axis === 'z') object.rotateZ(angle);
			else object.rotateX(angle);
		}
	}

	function prepareModel(scene: Object3D) {
		const instance = scene.clone(true);
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
		batchStaticRobotMeshes(instance);
		return instance;
	}

	$effect(() => {
		const scene = $robotGltf?.scene;
		if (!scene) return;
		const instance = prepareModel(scene);
		model = instance;
		renderedWheelAngle = 0;
	});

	$effect(() => {
		if (model) {
			buildCache(model);
		}
	});

	// Throttle for intake/outtake state change logging
	let lastLoggedIntaking = false;
	let lastLoggedOuttaking = false;

	useTask((delta) => {
		if (!model) return;

		// Log state changes for intake / outtake (not every frame)
		if (isIntaking !== lastLoggedIntaking) {
			console.log(`[RobotModel] isIntaking changed → ${isIntaking}`);
			lastLoggedIntaking = isIntaking;
		}
		if (isOuttaking !== lastLoggedOuttaking) {
			console.log(`[RobotModel] isOuttaking changed → ${isOuttaking}`);
			lastLoggedOuttaking = isOuttaking;
		}

		// 1. Climb wheels
		const climbConfig = cachedRollers['climb']?.config ?? rollerSettings.climb;
		const targetAngle = Number.isFinite(wheelAngle)
			? wheelAngle
			: climbing
				? renderedWheelAngle + delta * (climbConfig?.speed ?? 28)
				: renderedWheelAngle;
		const angleDelta = targetAngle - renderedWheelAngle;
		if (Math.abs(angleDelta) >= 1e-5) {
			rotateCachedNodes('climb', angleDelta);
			renderedWheelAngle = targetAngle;
		}

		// 2. Intake
		if (isIntaking && rollerSettings.intake) {
			rotateCachedNodes('intake', delta * (rollerSettings.intake.speed ?? 10));
		}

		// 3. Outtake + Transfer
		if (isOuttaking) {
			if (rollerSettings.outtake) {
				rotateCachedNodes('outtake', delta * (rollerSettings.outtake.speed ?? 22));
			}
			if (rollerSettings.transfer) {
				rotateCachedNodes('transfer', delta * (rollerSettings.transfer.speed ?? 8));
			}
		}
	});
</script>

{#if model}<T is={model} />{/if}
