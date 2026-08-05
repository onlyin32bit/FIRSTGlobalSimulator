<script lang="ts">
	import { T } from '@threlte/core';
	import { onMount } from 'svelte';
	import {
		BufferAttribute,
		BufferGeometry,
		Euler,
		Group,
		Matrix4,
		Mesh,
		MeshBasicMaterial,
		Vector3
	} from 'three';

	type Bounds = {
		min: [number, number, number];
		max: [number, number, number];
		center?: [number, number, number];
		halfExtents?: [number, number, number];
		axes?: [[number, number, number], [number, number, number], [number, number, number]];
	};
	type NamedBounds = Bounds & { id: string };

	let {
		colliders,
		triggers,
		boundary,
		activeTriggerIds = new Set<string>(),
		physicsUrl = null,
		showColliderAabbs = false
	}: {
		colliders: NamedBounds[];
		triggers: NamedBounds[];
		boundary: Bounds;
		activeTriggerIds?: Set<string>;
		physicsUrl?: string | null;
		showColliderAabbs?: boolean;
	} = $props();

	// The offline scene uses these authored triangle meshes directly. The
	// server also consumes the same physics JSON, but its compact AABB summary
	// is intentionally not suitable for visual debugging of rotated parts.
	const physicsMeshes = new Group();
	physicsMeshes.name = 'authored-field-collision-meshes';
	physicsMeshes.visible = Boolean(physicsUrl);

	onMount(() => {
		if (!physicsUrl) return;
		let disposed = false;

		const loadPhysicsMeshes = async () => {
			try {
				const response = await fetch(physicsUrl);
				if (!response.ok) throw new Error(`physics asset returned ${response.status}`);
				const data = (await response.json()) as {
					rootnode?: { children?: Array<{ name?: string; meshes?: number[]; transformation?: number[] }> };
					meshes?: Array<{ vertices?: number[]; faces?: number[][] }>;
				};
				if (disposed) return;

				const next = new Group();
				next.name = 'authored-field-collision-meshes';
				const material = new MeshBasicMaterial({
					color: '#38bdf8',
					wireframe: true,
					transparent: true,
					opacity: 0.48,
					depthWrite: false,
					depthTest: false
				});

				for (const node of data?.rootnode?.children ?? []) {
					const meshIndex = Number.isInteger(node?.meshes?.[0]) ? Number(node.meshes![0]) : null;
					const source = meshIndex === null ? null : data?.meshes?.[meshIndex];
					if (!source || !Array.isArray(source.vertices) || !Array.isArray(source.faces)) continue;

					const geometry = new BufferGeometry();
					geometry.setAttribute(
						'position',
						new BufferAttribute(new Float32Array(source.vertices), 3)
					);
					const indices: number[] = [];
					for (const face of source.faces) {
						if (!Array.isArray(face) || face.length < 3) continue;
						indices.push(face[0], face[1], face[2]);
						if (face.length === 4) indices.push(face[0], face[2], face[3]);
					}
					if (!indices.length) {
						geometry.dispose();
						continue;
					}
					geometry.setIndex(indices);
					const transform = Array.isArray(node?.transformation)
						? new Matrix4().fromArray(node.transformation).transpose()
						: new Matrix4();
					// Match Field.svelte / the offline ModelViewer exactly: apply
					// the authored node transform to the mesh, never an AABB.
					geometry.applyMatrix4(transform);
					geometry.computeBoundingSphere();
					const mesh = new Mesh(geometry, material.clone());
					mesh.name = node?.name ?? `field-collider-${next.children.length}`;
					mesh.renderOrder = 22;
					next.add(mesh);
				}

				physicsMeshes.clear();
				physicsMeshes.add(...next.children);
				physicsMeshes.visible = true;
			} catch (error) {
				console.warn('[field-collision-debug] unable to load authored physics mesh', error);
			}
		};

		void loadPhysicsMeshes();
		return () => {
			disposed = true;
			physicsMeshes.traverse((object) => {
				if (object instanceof Mesh) object.geometry.dispose();
			});
		};
	});

	$effect(() => {
		physicsMeshes.visible = Boolean(physicsUrl);
	});

	function center(bounds: Bounds): [number, number, number] {
		if (bounds.center) return bounds.center;
		return [
			(bounds.min[0] + bounds.max[0]) * 0.5,
			(bounds.min[1] + bounds.max[1]) * 0.5,
			(bounds.min[2] + bounds.max[2]) * 0.5
		];
	}

	function size(bounds: Bounds): [number, number, number] {
		if (bounds.halfExtents) {
			return bounds.halfExtents.map((extent) => Math.max(0.01, extent * 2)) as [number, number, number];
		}
		return [
			Math.max(0.01, bounds.max[0] - bounds.min[0]),
			Math.max(0.01, bounds.max[1] - bounds.min[1]),
			Math.max(0.01, bounds.max[2] - bounds.min[2])
		];
	}

	function rotation(bounds: Bounds): [number, number, number] {
		if (!bounds.axes) return [0, 0, 0];
		const matrix = new Matrix4().makeBasis(
			new Vector3(...bounds.axes[0]),
			new Vector3(...bounds.axes[1]),
			new Vector3(...bounds.axes[2])
		);
		const euler = new Euler().setFromRotationMatrix(matrix);
		return [euler.x, euler.y, euler.z];
	}
</script>

<!-- Exact authored collision meshes, matching the offline scene. -->
<T is={physicsMeshes} />

<!-- Optional simplified server volumes. Disabled by default; when enabled,
     oriented boxes use the same center/axes/half-extents as the live solver. -->
<T.Group>
	<!-- Green = the authoritative playable perimeter, derived from guard rail. -->
	<T.Mesh position={center(boundary)} scale={size(boundary)} renderOrder={19}>
		<T.BoxGeometry args={[1, 1, 1]} />
		<T.MeshBasicMaterial color="#22c55e" wireframe transparent opacity={0.9} depthWrite={false} />
	</T.Mesh>
	{#if showColliderAabbs}{#each colliders as collider (collider.id)}
			<T.Mesh position={center(collider)} rotation={rotation(collider)} scale={size(collider)} renderOrder={20}>
				<T.BoxGeometry args={[1, 1, 1]} />
				<T.MeshBasicMaterial
					color="#facc15"
					wireframe
					transparent
					opacity={0.72}
					depthWrite={false}
				/>
			</T.Mesh>
		{/each}{/if}
	{#each triggers as trigger (trigger.id)}
		{@const active = activeTriggerIds.has(trigger.id)}
		<T.Mesh position={center(trigger)} scale={size(trigger)} renderOrder={21}>
			<T.BoxGeometry args={[1, 1, 1]} />
			<T.MeshBasicMaterial
				color={active ? '#22c55e' : '#38bdf8'}
				wireframe
				transparent
				opacity={active ? 1 : 0.48}
				depthWrite={false}
			/>
		</T.Mesh>
	{/each}
</T.Group>
