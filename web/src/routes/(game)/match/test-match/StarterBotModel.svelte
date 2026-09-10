<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { useGltf, useMeshopt } from '@threlte/extras';
	import { untrack } from 'svelte';
	import { FrontSide, Mesh, MeshStandardMaterial, Object3D } from 'three';

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
		detailAssetUrl,
		climbing = false,
		wheelAngle = 0,
		isIntaking = false,
		isOuttaking = false,
		rollerSettings = defaultRollerSettings as RobotRollerSettings
	}: {
		assetUrl: string;
		detailAssetUrl?: string;
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
