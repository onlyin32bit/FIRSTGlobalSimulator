<script lang="ts">
	import { T } from '@threlte/core';
	import { Euler, Matrix4, Vector3 } from 'three';

	type Player = { id: string; x: number; y: number; z: number; yaw: number };
	type Frame = { positions: Float32Array; radius: number };
	type Collider = { center: [number, number, number]; half: [number, number, number]; axes: [[number, number, number], [number, number, number], [number, number, number]] };

	let {
		players,
		frame,
		width,
		height,
		length,
		physicsUrl,
		visible
	}: {
		players: Player[];
		frame: Frame;
		width: number;
		height: number;
		length: number;
		physicsUrl: string | undefined;
		visible: boolean;
	} = $props();
	let colliders = $state<Collider[]>([]);

	$effect(() => {
		if (!physicsUrl) return;
		const controller = new AbortController();
		fetch(physicsUrl, { signal: controller.signal })
			.then((response) => response.json())
			.then((asset) => {
				const result: Collider[] = [];
				let floor = Infinity;
				for (const node of asset.rootnode?.children ?? []) {
					const mesh = asset.meshes?.[node.meshes?.[0] ?? -1];
					const m = node.transformation;
					if (!mesh?.vertices || !m) continue;
					const points = [];
					const localMin = [Infinity, Infinity, Infinity];
					const localMax = [-Infinity, -Infinity, -Infinity];
					for (let i = 0; i < mesh.vertices.length; i += 3) {
						const x = mesh.vertices[i], y = mesh.vertices[i + 1], z = mesh.vertices[i + 2];
						localMin[0] = Math.min(localMin[0], x); localMin[1] = Math.min(localMin[1], y); localMin[2] = Math.min(localMin[2], z);
						localMax[0] = Math.max(localMax[0], x); localMax[1] = Math.max(localMax[1], y); localMax[2] = Math.max(localMax[2], z);
						points.push([m[0] * x + m[1] * y + m[2] * z + m[3], m[4] * x + m[5] * y + m[6] * z + m[7], m[8] * x + m[9] * y + m[10] * z + m[11]]);
					}
					const min = [Math.min(...points.map((p) => p[0])), Math.min(...points.map((p) => p[1])), Math.min(...points.map((p) => p[2]))];
					const max = [Math.max(...points.map((p) => p[0])), Math.max(...points.map((p) => p[1])), Math.max(...points.map((p) => p[2]))];
					floor = Math.min(floor, min[1]);
					const rawAxes = [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]];
					const axes = rawAxes.map((axis) => { const scale = Math.hypot(...axis); return axis.map((value) => value / Math.max(scale, 1e-6)); }) as Collider['axes'];
					const half = rawAxes.map((axis, i) => (localMax[i] - localMin[i]) * Math.hypot(...axis) / 2) as [number, number, number];
					const localCenter = localMin.map((value, i) => (value + localMax[i]) / 2);
					const center: [number, number, number] = [m[0] * localCenter[0] + m[1] * localCenter[1] + m[2] * localCenter[2] + m[3], m[4] * localCenter[0] + m[5] * localCenter[1] + m[6] * localCenter[2] + m[7], m[8] * localCenter[0] + m[9] * localCenter[1] + m[10] * localCenter[2] + m[11]];
					result.push({ center, half, axes });
				}
				colliders = result.map((collider) => ({ ...collider, center: [collider.center[0], collider.center[1] - floor - height / 2, collider.center[2]] }));
			})
			.catch(() => { colliders = []; });
		return () => controller.abort();
	});

	function ballTouchesRobot(ball: [number, number, number], player: Player) {
		const sin = Math.sin(player.yaw + Math.PI), cos = Math.cos(player.yaw + Math.PI);
		const volumes = colliders.length ? colliders : [{ center: [0, 0, 0] as [number, number, number], half: [width / 2, height / 2, length / 2] as [number, number, number], axes: [[1, 0, 0], [0, 1, 0], [0, 0, 1]] as Collider['axes'] }];
		return volumes.some((volume) => {
			const center = [player.x + cos * volume.center[0] + sin * volume.center[2], player.y + volume.center[1], player.z - sin * volume.center[0] + cos * volume.center[2]];
			const delta = [ball[0] - center[0], ball[1] - center[1], ball[2] - center[2]];
			const axes = volume.axes.map((axis) => [cos * axis[0] + sin * axis[2], axis[1], -sin * axis[0] + cos * axis[2]]);
			const local = axes.map((axis) => delta[0] * axis[0] + delta[1] * axis[1] + delta[2] * axis[2]);
			const closest = local.map((value, i) => Math.max(-volume.half[i], Math.min(volume.half[i], value)));
			return Math.hypot(...local.map((value, i) => value - closest[i])) < frame.radius;
		});
	}

	function ballPosition(index: number): [number, number, number] {
		return [frame.positions[index * 3], frame.positions[index * 3 + 1], frame.positions[index * 3 + 2]];
	}

	function colliderPose(player: Player, collider: Collider) {
		const yaw = player.yaw + Math.PI;
		const sin = Math.sin(yaw);
		const cos = Math.cos(yaw);
		const rotate = (axis: [number, number, number]): [number, number, number] => [
			cos * axis[0] + sin * axis[2],
			axis[1],
			-sin * axis[0] + cos * axis[2]
		];
		const worldAxes = collider.axes.map(rotate) as Collider['axes'];
		const matrix = new Matrix4().makeBasis(
			new Vector3(...worldAxes[0]),
			new Vector3(...worldAxes[1]),
			new Vector3(...worldAxes[2])
		);
		const euler = new Euler().setFromRotationMatrix(matrix);
		return {
			position: [
				player.x + cos * collider.center[0] + sin * collider.center[2],
				player.y + collider.center[1],
				player.z - sin * collider.center[0] + cos * collider.center[2]
			],
			rotation: [euler.x, euler.y, euler.z] as [number, number, number]
		};
	}
</script>

{#if visible}
	{#each players as player (player.id)}
		{#if colliders.length}
			{#each colliders as collider}
				{@const pose = colliderPose(player, collider)}
				<T.Mesh position={pose.position} rotation={pose.rotation}>
					<T.BoxGeometry args={[collider.half[0] * 2, collider.half[1] * 2, collider.half[2] * 2]} />
					<T.MeshBasicMaterial color="#22d3ee" wireframe transparent opacity={0.9} />
				</T.Mesh>
			{/each}
		{:else}
			<T.Mesh position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
				<T.BoxGeometry args={[width, height, length]} />
				<T.MeshBasicMaterial color="#f97316" wireframe transparent opacity={0.9} />
			</T.Mesh>
		{/if}
	{/each}
	{#each Array.from({ length: frame.positions.length / 3 }) as _, index (index)}
		{@const position = ballPosition(index)}
		{@const touching = players.some((player) => ballTouchesRobot(position, player))}
		<T.Mesh position={position}>
			<T.SphereGeometry args={[frame.radius * (touching ? 1.12 : 1.03), 8, 6]} />
			<T.MeshBasicMaterial color={touching ? '#ef4444' : '#facc15'} wireframe />
		</T.Mesh>
	{/each}
{/if}
