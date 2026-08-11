<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { HTML, useGltf, useMeshopt } from '@threlte/extras';
	import { BufferGeometry, Float32BufferAttribute, Matrix4, Mesh, MeshStandardMaterial, Object3D, Vector3, type MeshStandardMaterialParameters } from 'three';
	import type { MatchPhysics, MatchPlayer } from './match-protocol';

	let {
		player,
		physics,
		robotAssets,
		local,
		intake = 0,
		outtake = 0,
		climb = 0
	}: {
		player: MatchPlayer;
		physics: MatchPhysics;
		robotAssets: { visual: string; physics: string };
		local: boolean;
		intake?: number;
		outtake?: number;
		climb?: number;
	} = $props();

	type RobotPhysicsNode = { name?: string; transformation?: number[]; meshes?: number[]; children?: RobotPhysicsNode[] };
	type RobotPhysicsAsset = {
		rootnode?: { children?: RobotPhysicsNode[] };
		meshes?: Array<{ vertices?: number[]; normals?: number[]; faces?: number[][] }>;
	};

	const meshoptDecoder = useMeshopt();
	const robotGltf = useGltf(robotAssets.visual, { meshoptDecoder });
	let robotPhysicsAsset = $state<RobotPhysicsAsset | null>(null);

	type MechanismItem = {
		name: string;
		mesh: Mesh;
		spinAxis: Vector3;
		channel: 'intake' | 'outtake' | 'climb';
		speed: number;
	};

	let mechanismItems = $state<MechanismItem[]>([]);

	$effect(() => {
		const controller = new AbortController();
		fetch(robotAssets.physics, { signal: controller.signal })
			.then((response) => {
				if (!response.ok) throw new Error(`Unable to load ${robotAssets.physics}`);
				return response.json() as Promise<RobotPhysicsAsset>;
			})
			.then((asset) => {
				robotPhysicsAsset = asset;
				buildMechanismMeshes(asset);
			})
			.catch(() => {
				if (!controller.signal.aborted) robotPhysicsAsset = null;
			});
		return () => controller.abort();
	});

	function createMechanismMeshFromMatrix(
		node: RobotPhysicsNode,
		asset: RobotPhysicsAsset,
		worldMatrix: Matrix4,
		materialProps: MeshStandardMaterialParameters
	): Mesh | null {
		const meshIndex = node.meshes?.[0];
		const physMesh = Number.isInteger(meshIndex) ? asset.meshes?.[meshIndex!] : undefined;
		if (!physMesh?.vertices) return null;

		const geom = new BufferGeometry();
		geom.setAttribute('position', new Float32BufferAttribute(physMesh.vertices, 3));
		if (physMesh.normals && physMesh.normals.length > 0) {
			geom.setAttribute('normal', new Float32BufferAttribute(physMesh.normals, 3));
		} else {
			geom.computeVertexNormals();
		}

		if (physMesh.faces && physMesh.faces.length > 0) {
			const indices: number[] = [];
			for (const face of physMesh.faces) {
				indices.push(face[0], face[1], face[2]);
			}
			geom.setIndex(indices);
		}

		geom.computeBoundingBox();
		const center = new Vector3();
		geom.boundingBox?.getCenter(center);
		geom.center();

		const mat = new MeshStandardMaterial(materialProps);
		const mesh = new Mesh(geom, mat);
		mesh.castShadow = true;
		mesh.receiveShadow = true;

		worldMatrix.decompose(mesh.position, mesh.quaternion, mesh.scale);

		center.applyQuaternion(mesh.quaternion);
		center.multiply(mesh.scale);
		mesh.position.add(center);

		return mesh;
	}

	function getMechanismName(name: string | null | undefined): string | null {
		if (!name) return null;
		if (name === 'IntakeRoller' || name.startsWith('IntakeRoller')) return 'IntakeRoller';
		if (name === 'OuttakeRoller' || name.startsWith('OuttakeRoller')) return 'OuttakeRoller';
		if (name === 'TransferFlap' || name.startsWith('TransferFlap') || name.startsWith('Transfer Flap')) return 'TransferFlap';
		if (name === 'ClimbWheel1' || name.startsWith('ClimbWheel1')) return 'ClimbWheel1';
		if (name === 'ClimbWheel2' || name.startsWith('ClimbWheel2')) return 'ClimbWheel2';
		return null;
	}

	/** Recursively collect mechanism meshes, composing parent transforms down into children.
	 *  `inheritedName` is the nearest named ancestor — so child cylinders of an empty
	 *  "IntakeRoller" parent all get treated as IntakeRoller.
	 */
	function collectMechanismMeshes(
		node: RobotPhysicsNode,
		asset: RobotPhysicsAsset,
		parentMatrix: Matrix4,
		inheritedName: string | null,
		items: MechanismItem[]
	) {
		const nodeName = node.name ?? '';
		const directMech = getMechanismName(nodeName);
		const inheritedMech = getMechanismName(inheritedName);
		const effectiveMech = directMech || inheritedMech;

		// Compose this node's local transform with the parent's.
		const localMatrix = new Matrix4();
		if (node.transformation && node.transformation.length >= 16) {
			const a = node.transformation;
			localMatrix.set(
				a[0], a[1], a[2], a[3],
				a[4], a[5], a[6], a[7],
				a[8], a[9], a[10], a[11],
				a[12], a[13], a[14], a[15]
			);
		}
		const worldMatrix = parentMatrix.clone().multiply(localMatrix);

		if (effectiveMech) {
			const isIntake = effectiveMech === 'IntakeRoller';
			const isOuttake = effectiveMech === 'OuttakeRoller' || effectiveMech === 'TransferFlap';
			const isClimb = effectiveMech === 'ClimbWheel1' || effectiveMech === 'ClimbWheel2';

			// Only emit a mesh if this node actually has geometry.
			if (node.meshes && node.meshes.length > 0) {
				const channel: MechanismItem['channel'] = isOuttake ? 'outtake' : isClimb ? 'climb' : 'intake';
				// Intake is reversed (negative surface speed on server) so negate its visual speed.
				const speed = isOuttake ? 30 : isClimb ? 15 : 20;

				const matProps: MeshStandardMaterialParameters = isOuttake
					? { color: '#a3e635', roughness: 0.2, metalness: 0.6, emissive: '#65a30d', emissiveIntensity: 0.3 }
					: isClimb
					? { color: '#f97316', roughness: 0.4, metalness: 0.5 }
					: { color: '#84cc16', roughness: 0.3, metalness: 0.4, emissive: '#4d7c0f', emissiveIntensity: 0.2 };

				const spinAxis =
					isIntake || effectiveMech === 'OuttakeRoller'
						? new Vector3(0, 1, 0)
						: new Vector3(1, 0, 0);

				const mesh = createMechanismMeshFromMatrix(node, asset, worldMatrix, matProps);
				if (mesh) {
					items.push({ name: `${nodeName || effectiveMech}_${items.length}`, mesh, spinAxis, channel, speed });
				}
			}
		}

		// Recurse into children, passing this node's effective mechanism name down.
		if (node.children) {
			for (const child of node.children) {
				collectMechanismMeshes(child, asset, worldMatrix, effectiveMech || inheritedName, items);
			}
		}
	}

	function buildMechanismMeshes(asset: RobotPhysicsAsset) {
		if (!asset?.rootnode?.children) return;
		const items: MechanismItem[] = [];
		const identity = new Matrix4();
		for (const node of asset.rootnode.children) {
			collectMechanismMeshes(node, asset, identity, null, items);
		}
		mechanismItems = items;
	}

	useTask((delta) => {
		for (const item of mechanismItems) {
			const power =
				item.channel === 'intake'
					? intake
					: item.channel === 'outtake'
					? outtake
					: item.channel === 'climb'
					? climb
					: 0;

			if (Math.abs(power) > 0.01) {
				item.mesh.rotateOnAxis(item.spinAxis, item.speed * delta * power);
			}
		}
	});

	// bot.physics.json is authored in floor-relative robot coordinates. Use its
	// lowest collider vertex to keep the detailed GLB seated on the same floor
	// as the server's center-of-mass pose.
	function authoredFloorOffset(asset: RobotPhysicsAsset | null): number {
		if (!asset?.rootnode?.children || !asset.meshes) return 0;
		let minimum = Infinity;
		for (const node of asset.rootnode.children) {
			const matrix = node.transformation;
			const meshIndex = node.meshes?.[0];
			const mesh = Number.isInteger(meshIndex) ? asset.meshes[meshIndex!] : undefined;
			if (!matrix || matrix.length < 16 || !mesh?.vertices) continue;
			for (let index = 0; index + 2 < mesh.vertices.length; index += 3) {
				const x = mesh.vertices[index];
				const y = mesh.vertices[index + 1];
				const z = mesh.vertices[index + 2];
				minimum = Math.min(
					minimum,
					matrix[4] * x + matrix[5] * y + matrix[6] * z + matrix[7]
				);
			}
		}
		return Number.isFinite(minimum) ? -minimum : 0;
	}

	const floorOffset = $derived(authoredFloorOffset(robotPhysicsAsset));

	function configureRobotVisual(scene: Object3D): Object3D {
		const clone = scene.clone(true);
		clone.traverse((object) => {
			if (object instanceof Mesh) {
				object.castShadow = true;
				object.receiveShadow = true;
			}
		});
		return clone;
	}
</script>

<T.Group position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
	{#await $robotGltf then gltf}
		{#if gltf?.scene}
			<!-- The authored robot faces +Z; the simulator's forward direction is -Z. -->
			<T.Group position={[0, -physics.robotHeightM * 0.5 + floorOffset, 0]} rotation={[0, Math.PI, 0]}>
				<T is={configureRobotVisual(gltf.scene)} />
				{#each mechanismItems as item (item.name)}
					<T is={item.mesh} />
				{/each}
			</T.Group>
		{/if}
	{/await}

	<HTML position={[0, physics.robotHeightM * 0.5 + 0.55, 0]} center>
		<div
			class="pointer-events-none flex flex-col items-center gap-0.5 rounded-lg border border-white/10 bg-black/80 px-2.5 py-1 backdrop-blur-md shadow-lg font-sans text-xs"
		>
			<div class="flex items-center gap-1.5 font-bold tracking-wide text-white">
				<span class="size-2 rounded-full" style="background-color: {player.color}"></span>
				<span>{player.name}</span>
				{#if local}
					<span class="rounded bg-primary/20 px-1 py-0.2 text-[10px] font-mono text-primary font-semibold">YOU</span>
				{/if}
			</div>
		</div>
	</HTML>
</T.Group>
