<script lang="ts">
	import { T } from '@threlte/core';
	import { Euler, Matrix4, Vector3 } from 'three';

	type Player = {
		id: string;
		x: number;
		y: number;
		z: number;
		yaw: number;
		intakeRollerAngle: number;
	};
	type Frame = { positions: Float32Array; radius: number };
	type Collider = {
		id: string;
		center: [number, number, number];
		half: [number, number, number];
		axes: [[number, number, number], [number, number, number], [number, number, number]];
	};

	let {
		players,
		frame,
		width,
		height,
		length,
		physicsUrl,
		visible,
		intakeOnly = false,
		showBalls = true
	}: {
		players: Player[];
		frame: Frame;
		width: number;
		height: number;
		length: number;
		physicsUrl: string | undefined;
		visible: boolean;
		intakeOnly?: boolean;
		showBalls?: boolean;
	} = $props();
	let colliders = $state<Collider[]>([]);
	const displayedColliders = $derived(
		intakeOnly ? colliders.filter((collider) => collider.id === 'IntakeRoller') : colliders
	);

	function mechanismId(name: string | undefined, inherited: string | undefined) {
		const candidates = [name, inherited];
		if (candidates.some((candidate) => candidate === 'IntakeRoller' || candidate?.startsWith('IntakeRoller'))) return 'IntakeRoller';
		if (candidates.some((candidate) => candidate === 'OuttakeRoller' || candidate?.startsWith('OuttakeRoller'))) return 'OuttakeRoller';
		if (candidates.some((candidate) => candidate === 'TransferFlap' || candidate?.startsWith('TransferFlap') || candidate === 'Transfer Flap')) return 'TransferFlap';
		if (candidates.some((candidate) => candidate === 'ClimbWheel1' || candidate?.startsWith('ClimbWheel1'))) return 'ClimbWheel1';
		if (candidates.some((candidate) => candidate === 'ClimbWheel2' || candidate?.startsWith('ClimbWheel2'))) return 'ClimbWheel2';
		return undefined;
	}

	$effect(() => {
		if (!physicsUrl) return;
		const controller = new AbortController();
		fetch(physicsUrl, { signal: controller.signal })
			.then((response) => response.json())
			.then((asset: any) => {
				const result: Collider[] = [];
				let floor = Infinity;

				function collect(node: any, parentMatrix: Matrix4, inheritedId?: string) {
					const mechanism = mechanismId(node.name, inheritedId);
					const id = mechanism ?? node.name ?? inheritedId ?? 'unnamed';
					const localM = new Matrix4();
					if (node.transformation && node.transformation.length >= 16) {
						const a = node.transformation;
						localM.set(
							a[0], a[1], a[2], a[3],
							a[4], a[5], a[6], a[7],
							a[8], a[9], a[10], a[11],
							a[12], a[13], a[14], a[15]
						);
					}
					const worldM = parentMatrix.clone().multiply(localM);
					const meshIndex = node.meshes?.[0];
					const mesh = Number.isInteger(meshIndex) ? asset.meshes?.[meshIndex!] : undefined;

					if (mesh?.vertices) {
						const m = worldM.elements;
						const points = [];
						const localMin = [Infinity, Infinity, Infinity];
						const localMax = [-Infinity, -Infinity, -Infinity];
						for (let i = 0; i < mesh.vertices.length; i += 3) {
							const x = mesh.vertices[i], y = mesh.vertices[i + 1], z = mesh.vertices[i + 2];
							localMin[0] = Math.min(localMin[0], x); localMin[1] = Math.min(localMin[1], y); localMin[2] = Math.min(localMin[2], z);
							localMax[0] = Math.max(localMax[0], x); localMax[1] = Math.max(localMax[1], y); localMax[2] = Math.max(localMax[2], z);
							points.push([m[0] * x + m[4] * y + m[8] * z + m[12], m[1] * x + m[5] * y + m[9] * z + m[13], m[2] * x + m[6] * y + m[10] * z + m[14]]);
						}
						const min = [Math.min(...points.map((p) => p[0])), Math.min(...points.map((p) => p[1])), Math.min(...points.map((p) => p[2]))];
						const max = [Math.max(...points.map((p) => p[0])), Math.max(...points.map((p) => p[1])), Math.max(...points.map((p) => p[2]))];
						floor = Math.min(floor, min[1]);
						// Match the Rust pack loader exactly: authored transforms are
						// row-major, and each OBB axis comes from a matrix row.
						const rawAxes = [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]];
						const axes = rawAxes.map((axis) => { const scale = Math.hypot(...axis); return axis.map((value) => value / Math.max(scale, 1e-6)); }) as Collider['axes'];
						const half = rawAxes.map((axis, i) => (localMax[i] - localMin[i]) * Math.hypot(...axis) / 2) as [number, number, number];
						const localCenter = localMin.map((value, i) => (value + localMax[i]) / 2);
						const center: [number, number, number] = [m[0] * localCenter[0] + m[4] * localCenter[1] + m[8] * localCenter[2] + m[12], m[1] * localCenter[0] + m[5] * localCenter[1] + m[9] * localCenter[2] + m[13], m[2] * localCenter[0] + m[6] * localCenter[1] + m[10] * localCenter[2] + m[14]];
						result.push({ id, center, half, axes });
					}

					if (node.children) {
						for (const child of node.children) {
							collect(child, worldM, mechanism ?? inheritedId);
						}
					}
				}

				const identity = new Matrix4();
				for (const node of asset.rootnode?.children ?? []) {
					collect(node, identity);
				}
				colliders = result.map((collider) => ({ ...collider, center: [collider.center[0], collider.center[1] - floor - height / 2, collider.center[2]] }));
			})
			.catch(() => { colliders = []; });
		return () => controller.abort();
	});

	function ballTouchesRobot(ball: [number, number, number], player: Player) {
		const sin = Math.sin(player.yaw + Math.PI), cos = Math.cos(player.yaw + Math.PI);
		const volumes = colliders.length ? colliders : [{ id: 'RobotEnvelope', center: [0, 0, 0] as [number, number, number], half: [width / 2, height / 2, length / 2] as [number, number, number], axes: [[1, 0, 0], [0, 1, 0], [0, 0, 1]] as Collider['axes'] }];
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

	function colliderWorldAxes(player: Player, collider: Collider) {
		const yaw = player.yaw + Math.PI;
		const sin = Math.sin(yaw);
		const cos = Math.cos(yaw);
		const spin = collider.id === 'IntakeRoller' ? player.intakeRollerAngle : 0;
		const spinSin = Math.sin(spin);
		const spinCos = Math.cos(spin);
		// This is the same Y-axis Rodrigues rotation used by the server for
		// the intake roller's collision OBBs.
		const spinAxis = (axis: [number, number, number]): [number, number, number] => [
			axis[0] * spinCos + axis[2] * spinSin,
			axis[1],
			-axis[0] * spinSin + axis[2] * spinCos
		];
		const rotate = (axis: [number, number, number]): [number, number, number] => [
			cos * axis[0] + sin * axis[2],
			axis[1],
			-sin * axis[0] + cos * axis[2]
		];
		return collider.axes.map((axis) => rotate(spinAxis(axis))) as Collider['axes'];
	}

	function colliderPose(player: Player, collider: Collider) {
		const yaw = player.yaw + Math.PI;
		const sin = Math.sin(yaw);
		const cos = Math.cos(yaw);
		const worldAxes = colliderWorldAxes(player, collider);
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
			] as [number, number, number],
			rotation: [euler.x, euler.y, euler.z] as [number, number, number]
		};
	}

	function intakeCylinderPose(player: Player, collider: Collider) {
		const yaw = player.yaw + Math.PI;
		const sin = Math.sin(yaw);
		const cos = Math.cos(yaw);
		const axialAxis = collider.half.reduce(
			(longest, extent, index) => (extent > collider.half[longest] ? index : longest),
			0
		);
		const radialAxes = [0, 1, 2].filter((axis) => axis !== axialAxis);
		const worldAxes = colliderWorldAxes(player, collider);
		const matrix = new Matrix4().makeBasis(
			new Vector3(...worldAxes[radialAxes[0]]),
			new Vector3(...worldAxes[axialAxis]),
			new Vector3(...worldAxes[radialAxes[1]])
		);
		const euler = new Euler().setFromRotationMatrix(matrix);
		return {
			position: [
				player.x + cos * collider.center[0] + sin * collider.center[2],
				player.y + collider.center[1],
				player.z - sin * collider.center[0] + cos * collider.center[2]
			] as [number, number, number],
			rotation: [euler.x, euler.y, euler.z] as [number, number, number],
			radius: (collider.half[radialAxes[0]] + collider.half[radialAxes[1]]) * 0.5,
			height: collider.half[axialAxis] * 2
		};
	}
</script>

{#if visible}
	{#each players as player (player.id)}
		{#if displayedColliders.length}
			{#each displayedColliders as collider}
				{@const isIntakeCylinder = collider.id === 'IntakeRoller'}
				{@const boxPose = colliderPose(player, collider)}
				{@const cylinderPose = intakeCylinderPose(player, collider)}
				<T.Mesh
					position={isIntakeCylinder ? cylinderPose.position : boxPose.position}
					rotation={isIntakeCylinder ? cylinderPose.rotation : boxPose.rotation}
					renderOrder={100}
				>
					{#if isIntakeCylinder}
						<T.CylinderGeometry args={[cylinderPose.radius, cylinderPose.radius, cylinderPose.height, 16]} />
					{:else}
						<T.BoxGeometry args={[collider.half[0] * 2, collider.half[1] * 2, collider.half[2] * 2]} />
					{/if}
					<T.MeshBasicMaterial
						color={intakeOnly ? '#d946ef' : '#22d3ee'}
						wireframe={!intakeOnly}
						transparent
						opacity={intakeOnly ? 0.38 : 0.9}
						depthWrite={false}
						depthTest={!intakeOnly}
					/>
				</T.Mesh>
			{/each}
		{:else if !intakeOnly}
			<T.Mesh position={[player.x, player.y, player.z]} rotation={[0, player.yaw, 0]}>
				<T.BoxGeometry args={[width, height, length]} />
				<T.MeshBasicMaterial color="#f97316" wireframe transparent opacity={0.9} />
			</T.Mesh>
		{/if}
	{/each}
	{#if showBalls}
		{#each Array.from({ length: frame.positions.length / 3 }) as _, index (index)}
			{@const position = ballPosition(index)}
			{@const touching = players.some((player) => ballTouchesRobot(position, player))}
			<T.Mesh position={position}>
				<T.SphereGeometry args={[frame.radius * (touching ? 1.12 : 1.03), 8, 6]} />
				<T.MeshBasicMaterial color={touching ? '#ef4444' : '#facc15'} wireframe />
			</T.Mesh>
		{/each}
	{/if}
{/if}
