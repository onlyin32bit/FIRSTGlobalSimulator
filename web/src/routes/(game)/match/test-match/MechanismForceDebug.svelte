<script lang="ts">
	import { T } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import { Vector3, Matrix4 } from 'three';

	type Player = {
		id: string;
		x: number;
		y: number;
		z: number;
		yaw: number;
	};

	type ZoneInfo = {
		id: 'IntakeZone' | 'TransferZone' | 'OuttakeZone';
		center: [number, number, number];
		localXAxis: [number, number, number]; // Unit vector of local X rotation axis in robot space
		feedDirection: [number, number, number]; // Authored feed direction into crack/chute in robot space
		sampleContact: [number, number, number]; // Contact point relative to center in robot space
	};

	let {
		players,
		semanticsUrl,
		visible = true
	}: {
		players: Player[];
		semanticsUrl: string | undefined;
		visible: boolean;
	} = $props();

	let zones = $state<ZoneInfo[]>([]);

	$effect(() => {
		if (!semanticsUrl) return;
		const controller = new AbortController();
		fetch(semanticsUrl, { signal: controller.signal })
			.then((res) => res.json())
			.then((asset: any) => {
				const foundZones: ZoneInfo[] = [];

				function inspectNode(node: any, parentMatrix: Matrix4) {
					const nodeName = node.name ?? '';
					let zoneId: ZoneInfo['id'] | null = null;
					if (nodeName === 'IntakeZone' || nodeName.startsWith('IntakeZone')) zoneId = 'IntakeZone';
					else if (nodeName === 'TransferZone' || nodeName.startsWith('TransferZone')) zoneId = 'TransferZone';
					else if (nodeName === 'OuttakeZone' || nodeName.startsWith('OuttakeZone')) zoneId = 'OuttakeZone';

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

					if (zoneId) {
						const m = worldM.elements;
						// Column 0 is local X rotation axis in robot space
						const rawX = [m[0], m[4], m[8]];
						const lenX = Math.hypot(...rawX) || 1e-6;
						const localXAxis: [number, number, number] = [rawX[0] / lenX, rawX[1] / lenX, rawX[2] / lenX];

						// Column 2 is local Z axis -> feed direction into crack is -Z
						const rawZ = [m[2], m[6], m[10]];
						const lenZ = Math.hypot(...rawZ) || 1e-6;
						const feedDirection: [number, number, number] = [-rawZ[0] / lenZ, -rawZ[1] / lenZ, -rawZ[2] / lenZ];

						// Center of zone in robot local space
						const center: [number, number, number] = [m[12], m[13], m[14]];

						// Contact point relative to center where ball enters gap/crack
						let sampleContact: [number, number, number] = [0, 0, 0];
						if (zoneId === 'IntakeZone') {
							// Directly under roller in the intake mouth crack
							sampleContact = [0, -0.05, 0.04];
						} else if (zoneId === 'TransferZone') {
							// Inside transfer chute path
							sampleContact = [0, 0.02, -0.02];
						} else if (zoneId === 'OuttakeZone') {
							// At outtake shooter mouth
							sampleContact = [0, 0.04, -0.04];
						}

						foundZones.push({ id: zoneId, center, localXAxis, feedDirection, sampleContact });
					}

					if (node.children) {
						for (const child of node.children) {
							inspectNode(child, worldM);
						}
					}
				}

				const identity = new Matrix4();
				for (const child of asset.rootnode?.children ?? []) {
					inspectNode(child, identity);
				}

				zones = foundZones;
			})
			.catch(() => {
				zones = [];
			});

		return () => controller.abort();
	});

	function computeZoneForce(player: Player, zone: ZoneInfo) {
		const yaw = player.yaw + Math.PI;
		const sin = Math.sin(yaw);
		const cos = Math.cos(yaw);

		// 1. Roller Center in world space
		const centerWorld = new Vector3(
			player.x + cos * zone.center[0] + sin * zone.center[2],
			player.y + zone.center[1],
			player.z - sin * zone.center[0] + cos * zone.center[2]
		);

		// 2. Local X axis in world space (omega_world)
		const omegaWorld = new Vector3(
			cos * zone.localXAxis[0] + sin * zone.localXAxis[2],
			zone.localXAxis[1],
			-sin * zone.localXAxis[0] + cos * zone.localXAxis[2]
		).normalize();

		// 3. Side Profile Feed Direction in world space (straight into crack under cylinder)
		const sideFeedDirection = new Vector3(
			cos * zone.feedDirection[0] + sin * zone.feedDirection[2],
			zone.feedDirection[1],
			-sin * zone.feedDirection[0] + cos * zone.feedDirection[2]
		).normalize();

		// 4. Ball Contact Position
		const rWorld = new Vector3(
			cos * zone.sampleContact[0] + sin * zone.sampleContact[2],
			zone.sampleContact[1],
			-sin * zone.sampleContact[0] + cos * zone.sampleContact[2]
		);
		const ballPosition = centerWorld.clone().add(rWorld);

		// 5. Unrestricted 3D Tangent (for comparison)
		const rawTangentVector = new Vector3().crossVectors(omegaWorld, rWorld);
		const raw3DTangent = rawTangentVector.lengthSq() > 1e-8 ? rawTangentVector.clone().normalize() : sideFeedDirection.clone();

		// 6. FINAL Force Direction (Straight into crack / feed path)
		const finalForceDirection = sideFeedDirection.clone();

		let color = '#22c55e'; // Green for Intake
		let behaviorText = 'Intake → straight into crack under cylinder / into robot';
		if (zone.id === 'TransferZone') {
			color = '#eab308'; // Yellow for Transfer
			behaviorText = 'Transfer → straight through transfer path';
		} else if (zone.id === 'OuttakeZone') {
			color = '#f97316'; // Orange for Outtake
			behaviorText = 'Outtake → straight out of robot';
		}

		return {
			centerWorld,
			omegaWorld,
			ballPosition,
			raw3DTangent,
			finalForceDirection,
			color,
			behaviorText,
			rawStr: `[${raw3DTangent.x.toFixed(2)}, ${raw3DTangent.y.toFixed(2)}, ${raw3DTangent.z.toFixed(2)}]`,
			finalStr: `[${finalForceDirection.x.toFixed(2)}, ${finalForceDirection.y.toFixed(2)}, ${finalForceDirection.z.toFixed(2)}]`
		};
	}
</script>

{#if visible}
	{#each players as player (player.id)}
		{#each zones as zone (zone.id)}
			{@const data = computeZoneForce(player, zone)}

			<!-- 1. Roller Center (Red Sphere) -->
			<T.Mesh position={[data.centerWorld.x, data.centerWorld.y, data.centerWorld.z]}>
				<T.SphereGeometry args={[0.025, 12, 12]} />
				<T.MeshBasicMaterial color="#ef4444" />
			</T.Mesh>

			<!-- 2. Roller Local X Axis (Cyan Arrow) -->
			<T.ArrowHelper
				args={[
					data.omegaWorld,
					data.centerWorld,
					0.2,
					0x06b6d4,
					0.04,
					0.02
				]}
			/>

			<!-- 3. Ball Contact Position (Yellow Sphere) -->
			<T.Mesh position={[data.ballPosition.x, data.ballPosition.y, data.ballPosition.z]}>
				<T.SphereGeometry args={[0.035, 12, 12]} />
				<T.MeshBasicMaterial color="#facc15" wireframe />
			</T.Mesh>

			<!-- 4. Unrestricted 3D Tangent (Thin Blue Arrow) -->
			<T.ArrowHelper
				args={[
					data.raw3DTangent,
					data.ballPosition,
					0.22,
					0x3b82f6,
					0.04,
					0.02
				]}
			/>

			<!-- 5. FINAL Force Vector (Thick Colored Arrow - straight into crack under cylinder) -->
			<T.ArrowHelper
				args={[
					data.finalForceDirection,
					data.ballPosition,
					0.38,
					data.color,
					0.09,
					0.05
				]}
			/>

			<!-- Detailed 3D Scene Badge -->
			<HTML position={[data.ballPosition.x, data.ballPosition.y + 0.14, data.ballPosition.z]} center>
				<div class="pointer-events-none rounded border border-white/20 bg-black/90 p-2 font-mono text-[10px] text-white shadow-xl backdrop-blur">
					<div class="flex items-center justify-between border-b border-white/10 pb-1">
						<span class="font-bold text-xs" style="color: {data.color}">{zone.id}</span>
						<span class="ml-2 font-bold text-emerald-400">{data.behaviorText}</span>
					</div>
					<div class="mt-1 space-y-0.5 text-gray-300">
						<div>Unrestricted 3D Tangent: <span class="text-blue-400 font-bold">{data.rawStr}</span></div>
						<div>FINAL Feed Force (into crack): <span class="font-bold" style="color: {data.color}">{data.finalStr}</span></div>
					</div>
				</div>
			</HTML>
		{/each}
	{/each}
{/if}
