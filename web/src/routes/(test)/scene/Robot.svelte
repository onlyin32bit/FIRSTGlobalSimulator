<script lang="ts">
	import { T, useTask } from '@threlte/core';
	import { RigidBody, Collider } from '@threlte/rapier';
	import type { RigidBody as RapierRigidBody } from '@dimforge/rapier3d-compat';
	import { Vector3, Quaternion } from 'three';
	import { HTML } from '@threlte/extras';
	import { onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { robotTelemetry } from './telemetry';
	import { robotPhysicsState, robotStorageMap, showRobotTagsStore } from './stores';

	let {
		resetTrigger = 0,
		spawnPos = [0, 1.5, 3.15],
		controllerEnabled = true,
		slotId = 'red-1',
		slotName = 'Red 1',
		alliance = 'red' as 'red' | 'blue',
		isAi = false,
		color = undefined as string | undefined
	} = $props();

	let aiTimer = 0;
	let aiForwardTarget = 0;
	let aiTurnTarget = 0;
	let showTagsGamepad = $state(false);

	let rigidBody: RapierRigidBody | undefined = $state();

	// CCD handles fast contacts; these values keep the robot responsive without
	// letting it push through a pile of game pieces in one physics step.
	const MAX_SPEED = 4.0;
	const MAX_TURN = 7.0;
	const ACCEL = 12.0;
	const TURN_ACCEL = 40.0;
	const DRIVE_RESPONSE = 0.55;
	const LATERAL_GRIP = 0.8;
	const MAX_DRIVE_IMPULSE = 6.0;

	let currentLinSpeed = 0;
	let currentAngSpeed = 0;
	let stuckTimer = 0;
	let autoUnstickCount = 0;
	let maxContactForce = 0;
	const activeContacts = new SvelteMap<number, string>();
	const keys = {
		forward: false,
		reverse: false,
		left: false,
		right: false,
		intake: false,
		shoot: false,
		transfer: false
	};

	function clearKeyboardInput() {
		for (const key of Object.keys(keys) as Array<keyof typeof keys>) {
			keys[key] = false;
		}
	}

	onMount(() => {
		const setKey = (event: KeyboardEvent, pressed: boolean) => {
			switch (event.key.toLowerCase()) {
				case 'arrowup':
					keys.forward = pressed;
					break;
				case 'arrowdown':
					keys.reverse = pressed;
					break;
				case 'arrowleft':
					keys.left = pressed;
					break;
				case 'arrowright':
					keys.right = pressed;
					break;
				case 'e':
					keys.intake = pressed;
					break;
				case 'q':
					keys.shoot = pressed;
					break;
				case 'f':
					keys.transfer = pressed;
					break;
				default:
					return;
			}

			event.preventDefault();
		};

		const handleKeyDown = (event: KeyboardEvent) => setKey(event, true);
		const handleKeyUp = (event: KeyboardEvent) => setKey(event, false);

		window.addEventListener('keydown', handleKeyDown);
		window.addEventListener('keyup', handleKeyUp);
		window.addEventListener('blur', clearKeyboardInput);

		return () => {
			window.removeEventListener('keydown', handleKeyDown);
			window.removeEventListener('keyup', handleKeyUp);
			window.removeEventListener('blur', clearKeyboardInput);
		};
	});

	const spawnRotationY = $derived(alliance === 'blue' ? Math.PI / 2 : -Math.PI / 2);
	const spawnQuat = $derived(
		alliance === 'blue'
			? { x: 0, y: 0.7071, z: 0, w: 0.7071 }
			: { x: 0, y: -0.7071, z: 0, w: 0.7071 }
	);

	$effect(() => {
		if (resetTrigger > 0 && rigidBody) {
			rigidBody.setTranslation({ x: spawnPos[0], y: spawnPos[1], z: spawnPos[2] }, true);
			// Face inwards toward field center (Blue: +90°, Red: -90°)
			rigidBody.setRotation(spawnQuat, true);
			rigidBody.setLinvel({ x: 0, y: 0, z: 0 }, true);
			rigidBody.setAngvel({ x: 0, y: 0, z: 0 }, true);
			rigidBody.wakeUp();
			currentLinSpeed = 0;
			currentAngSpeed = 0;
			stuckTimer = 0;
			autoUnstickCount = 0;
			maxContactForce = 0;
			activeContacts.clear();
		}
	});

	let lastSpeedForTelemetry = 0;
	let smoothedFps = 60;
	let timeSinceLastTelemetry = 0;

	let rollerRotation = $state(0);

	type PhysicsEvent = {
		targetCollider: { handle: number; userData?: unknown };
		targetRigidBody: RapierRigidBody | null;
	};

	function contactLabel(event: PhysicsEvent) {
		const colliderData = event.targetCollider.userData as { fieldColliderId?: string } | undefined;
		const bodyData = event.targetRigidBody?.userData as { fieldColliderId?: string } | undefined;
		return (
			colliderData?.fieldColliderId ??
			bodyData?.fieldColliderId ??
			`collider-${event.targetCollider.handle}`
		);
	}

	function logCollision(event: PhysicsEvent, phase: 'enter' | 'exit') {
		const label = contactLabel(event);
		console.debug(`[robot-physics] collision ${phase}`, {
			collider: label,
			position: rigidBody?.translation(),
			velocity: rigidBody?.linvel(),
			angularVelocity: rigidBody?.angvel(),
			activeContacts: [...activeContacts.values()]
		});
	}

	function handleCollisionEnter(event: PhysicsEvent) {
		activeContacts.set(event.targetCollider.handle, contactLabel(event));
		logCollision(event, 'enter');
	}

	function handleCollisionExit(event: PhysicsEvent) {
		activeContacts.delete(event.targetCollider.handle);
		logCollision(event, 'exit');
	}

	function handleContact(event: PhysicsEvent & { totalForceMagnitude: number }) {
		maxContactForce = Math.max(maxContactForce, event.totalForceMagnitude);
	}

	useTask((delta) => {
		if (!rigidBody) return;

		const linvel = rigidBody.linvel();
		const angvel = rigidBody.angvel();
		const pos = rigidBody.translation();
		const mass = rigidBody.mass();

		// --- TELEMETRY ---
		const currentFps = delta > 0 ? 1 / delta : 0;
		smoothedFps = smoothedFps * 0.95 + currentFps * 0.05;

		const currentSpeedMag = Math.sqrt(linvel.x * linvel.x + linvel.z * linvel.z);
		const accel = (currentSpeedMag - lastSpeedForTelemetry) / delta;
		lastSpeedForTelemetry = currentSpeedMag;

		// --- TELEMETRY THROTTLING ---
		// Updating Svelte DOM text nodes 60 times a second blocks the main thread with Layout/Paint overhead!
		// We throttle the UI updates to 10Hz to free up the WebGL and Physics loops.
		timeSinceLastTelemetry += delta;
		const shouldPublishTelemetry = timeSinceLastTelemetry > 0.1;
		if (shouldPublishTelemetry) timeSinceLastTelemetry = 0;
		// -----------------

		let forwardInput = 0;
		let turnInput = 0;
		let intakeBtn = false;
		let shootBtn = false;
		let transferBtn = false;

		const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
		let gp: Gamepad | null = null;
		for (let i = 0; i < gamepads.length; i++) {
			if (gamepads[i] && gamepads[i]!.connected) {
				gp = gamepads[i];
				break;
			}
		}

		if (controllerEnabled && gp) {
			// 1. Forward / Reverse Input (Left Stick Y axis[1] or D-Pad Up/Down)
			const axisForward = gp.axes.length > 1 && Math.abs(gp.axes[1]) > 0.1 ? -gp.axes[1] : 0;
			const dpadForward = (gp.buttons[12]?.pressed ? 1 : 0) - (gp.buttons[13]?.pressed ? 1 : 0);
			const rawForward = axisForward !== 0 ? axisForward : dpadForward;

			// 2. Turning / Steering Input (Right Stick X axis[2] or D-Pad Left/Right)
			const rightAxisTurn = gp.axes.length > 2 && Math.abs(gp.axes[2]) > 0.1 ? -gp.axes[2] : 0;
			const dpadTurn = (gp.buttons[14]?.pressed ? 1 : 0) - (gp.buttons[15]?.pressed ? 1 : 0);
			const rawTurn = rightAxisTurn !== 0 ? rightAxisTurn : dpadTurn;

			forwardInput = rawForward;
			turnInput = rawTurn;

			intakeBtn =
				gp.buttons[5]?.pressed || gp.buttons[7]?.pressed || (gp.axes[5] ?? 0) > 0.3 || false;
			shootBtn =
				gp.buttons[4]?.pressed || gp.buttons[6]?.pressed || (gp.axes[4] ?? 0) > 0.3 || false;
			transferBtn = gp.buttons[0]?.pressed || gp.buttons[2]?.pressed || false;
			showTagsGamepad = gp.buttons[8]?.pressed || false;
		}

		const keyboardForward = controllerEnabled ? Number(keys.forward) - Number(keys.reverse) : 0;
		const keyboardTurn = controllerEnabled ? Number(keys.left) - Number(keys.right) : 0;
		forwardInput = Math.max(-1, Math.min(1, forwardInput + keyboardForward));
		turnInput = Math.max(-1, Math.min(1, turnInput + keyboardTurn));
		if (controllerEnabled) {
			intakeBtn ||= keys.intake;
			shootBtn ||= keys.shoot;
			transferBtn ||= keys.transfer;
		} else if (isAi) {
			aiTimer += delta;
			if (aiTimer > 2.2 + Math.random()) {
				aiForwardTarget = (Math.random() - 0.2) * 0.7;
				aiTurnTarget = (Math.random() - 0.5) * 1.0;
				aiTimer = 0;
			}
			forwardInput = aiForwardTarget;
			turnInput = aiTurnTarget;
		}

		let leftPower = forwardInput - turnInput;
		let rightPower = forwardInput + turnInput;

		const maxPower = Math.max(Math.abs(leftPower), Math.abs(rightPower));
		if (maxPower > 1.0) {
			leftPower /= maxPower;
			rightPower /= maxPower;
		}

		const driveSpeed = (leftPower + rightPower) / 2;
		const turnSpeed = (rightPower - leftPower) / 2;

		const targetSpeed = driveSpeed * MAX_SPEED;
		const targetTurn = turnSpeed * MAX_TURN;

		const rot = rigidBody.rotation();
		const quat = new Quaternion(rot.x, rot.y, rot.z, rot.w);
		const forwardVec = new Vector3(0, 0, -1).applyQuaternion(quat);
		const rightVec = new Vector3(1, 0, 0).applyQuaternion(quat);

		const forwardSpeed = linvel.x * forwardVec.x + linvel.z * forwardVec.z;
		const lateralSpeed = linvel.x * rightVec.x + linvel.z * rightVec.z;

		// Check if robot is stuck against an obstacle (driver giving input but movement/rotation is blocked)
		const hasDriverInput = Math.abs(forwardInput) > 0.1 || Math.abs(turnInput) > 0.1;
		if (hasDriverInput) {
			rigidBody.wakeUp();
		}
		const isStuckAgainstObstacle =
			hasDriverInput && Math.abs(forwardSpeed) < 0.15 && Math.abs(angvel.y) < 0.4;

		// Instant reverse response when blocked by an obstacle (e.g. brace):
		// If input direction flips while blocked or ramping, snap currentLinSpeed to targetSpeed directly
		// so reverse force is applied immediately instead of continuing to push into the brace.
		if (Math.sign(targetSpeed) !== Math.sign(currentLinSpeed) && targetSpeed !== 0) {
			currentLinSpeed = targetSpeed;
		} else if (forwardInput === 0 && Math.abs(forwardSpeed) < 0.1) {
			currentLinSpeed = 0;
		}

		if (currentLinSpeed < targetSpeed) {
			currentLinSpeed = Math.min(targetSpeed, currentLinSpeed + ACCEL * delta);
		} else if (currentLinSpeed > targetSpeed) {
			currentLinSpeed = Math.max(targetSpeed, currentLinSpeed - ACCEL * delta);
		}

		if (currentAngSpeed < targetTurn) {
			currentAngSpeed = Math.min(targetTurn, currentAngSpeed + TURN_ACCEL * delta);
		} else if (currentAngSpeed > targetTurn) {
			currentAngSpeed = Math.max(targetTurn, currentAngSpeed - TURN_ACCEL * delta);
		}

		// Auto-unstick helper: if driver is giving input but robot is trapped in a wedge/brace for > 0.35s,
		// apply a strong pulse backwards, upwards & rotational pop to break wedging friction.
		if (isStuckAgainstObstacle) {
			stuckTimer += delta;
			if (stuckTimer > 0.35) {
				const popDir = forwardInput > 0.1 ? -1 : forwardInput < -0.1 ? 1 : -1;
				rigidBody.applyImpulse(
					{
						x: popDir * forwardVec.x * 5.0 + (Math.random() - 0.5) * 2.0,
						y: 2.5,
						z: popDir * forwardVec.z * 5.0 + (Math.random() - 0.5) * 2.0
					},
					true
				);
				rigidBody.applyTorqueImpulse(
					{
						x: 0,
						y: autoUnstickCount % 2 === 0 ? 6.0 : -6.0,
						z: 0
					},
					true
				);
				autoUnstickCount += 1;
				console.debug('[robot-physics] auto-unstick impulse', {
					count: autoUnstickCount,
					stuckTime: stuckTimer,
					position: rigidBody.translation(),
					velocity: rigidBody.linvel(),
					contacts: [...activeContacts.values()]
				});
				stuckTimer = 0;
			}
		} else {
			stuckTimer = 0;
		}

		// Sync robot state for the Intake/Outtake system (only for the active controlled robot)
		if (controllerEnabled) {
			robotPhysicsState.set({
				pos: { x: pos.x, y: pos.y, z: pos.z },
				vel: { x: linvel.x, y: linvel.y, z: linvel.z },
				forward: { x: forwardVec.x, y: forwardVec.y, z: forwardVec.z },
				isIntakeActive: intakeBtn,
				isShootActive: shootBtn,
				isTransferActive: transferBtn
			});
		}

		if (intakeBtn) {
			rollerRotation -= delta * 30; // Spin rapidly inwards
		} else if (transferBtn) {
			rollerRotation += delta * 30; // Spin rapidly outwards for front transfer!
		}

		// Ease lateral scrub when turning or stuck against an obstacle so robot pivots free easily
		const effectiveLateralGrip = isStuckAgainstObstacle ? 0.15 : LATERAL_GRIP;

		const forwardImpulse = (currentLinSpeed - forwardSpeed) * mass * DRIVE_RESPONSE;
		const lateralImpulse = -lateralSpeed * mass * effectiveLateralGrip;
		let impulseX = forwardVec.x * forwardImpulse + rightVec.x * lateralImpulse;
		let impulseZ = forwardVec.z * forwardImpulse + rightVec.z * lateralImpulse;

		// CLAMP THE MAX IMPULSE! Boost available drive impulse when dislodging from an obstacle
		const currentMaxDriveImpulse = isStuckAgainstObstacle ? 12.0 : MAX_DRIVE_IMPULSE;
		const impulseMag = Math.sqrt(impulseX * impulseX + impulseZ * impulseZ);
		if (impulseMag > currentMaxDriveImpulse) {
			impulseX = (impulseX / impulseMag) * currentMaxDriveImpulse;
			impulseZ = (impulseZ / impulseMag) * currentMaxDriveImpulse;
		}

		if (shouldPublishTelemetry) {
			robotTelemetry.set({
				x: pos.x,
				y: pos.y,
				z: pos.z,
				speed: currentSpeedMag,
				turnRate: angvel.y,
				accel,
				fps: smoothedFps,
				forwardSpeed,
				requestedForwardSpeed: currentLinSpeed,
				driveImpulse: impulseMag,
				contactForce: maxContactForce,
				contactCount: activeContacts.size,
				contacts: [...activeContacts.values()],
				stuckTime: stuckTimer,
				autoUnstickCount
			});
			maxContactForce = 0;
		}

		rigidBody.applyImpulse(
			{
				x: impulseX,
				y: 0,
				z: impulseZ
			},
			true
		);

		const deltaW = currentAngSpeed - angvel.y;
		const inertia = (mass / 12.0) * (0.45 * 0.45 + 0.45 * 0.45);
		const angularResponse = 1.2;

		let torqueImpulse = deltaW * inertia * angularResponse;

		// Boost turning torque when stuck so driver can easily pivot away from collision surfaces
		const maxTorqueImpulse = isStuckAgainstObstacle ? 10.0 : 4.0;
		if (Math.abs(torqueImpulse) > maxTorqueImpulse) {
			torqueImpulse = Math.sign(torqueImpulse) * maxTorqueImpulse;
		}

		rigidBody.applyTorqueImpulse(
			{
				x: 0,
				y: torqueImpulse,
				z: 0
			},
			true
		);
	});
</script>

<T.Group position={[spawnPos[0], spawnPos[1], spawnPos[2]]} rotation={[0, spawnRotationY, 0]}>
	<RigidBody
		bind:rigidBody
		type="dynamic"
		enabledRotations={[false, true, false]}
		enabledTranslations={[true, true, true]}
		linearDamping={0.15}
		angularDamping={0.8}
		ccd={true}
		oncollisionenter={handleCollisionEnter}
		oncollisionexit={handleCollisionExit}
		oncontact={handleContact}
	>
		<!-- Rounded, low-friction edges act more like wheels and do not catch on field seams. -->
		<T.Group position={[0, 0.15, 0]}>
			<Collider
				shape="roundCuboid"
				args={[0.275, 0.14, 0.325, 0.05]}
				friction={0.25}
				restitution={0.02}
				mass={18.0}
			/>
		</T.Group>

		<T.Mesh castShadow receiveShadow position={[0, 0.15, 0]}>
			<T.BoxGeometry args={[0.5, 0.3, 0.6]} />
			<T.MeshStandardMaterial
				color={color || (alliance === 'red' ? '#dc2626' : '#2563eb')}
				roughness={0.4}
				metalness={0.2}
			/>
		</T.Mesh>

		<!-- Alliance Bumper Accent -->
		<T.Mesh position={[0, 0.05, 0]}>
			<T.BoxGeometry args={[0.52, 0.1, 0.62]} />
			<T.MeshStandardMaterial color={alliance === 'red' ? '#991b1b' : '#1e3a8a'} roughness={0.6} />
		</T.Mesh>

		<!-- Active Intake Mechanism (Compliant Roller) -->
		<T.Group position={[0, 0.05, -0.35]} rotation={[rollerRotation, 0, 0]}>
			<!-- The main roller axle -->
			<T.Mesh rotation={[0, 0, Math.PI / 2]} castShadow>
				<T.CylinderGeometry args={[0.04, 0.04, 0.48, 16]} />
				<T.MeshStandardMaterial color="#222222" roughness={0.9} />
			</T.Mesh>

			<!-- Green compliant wheels on the axle -->
			{#each [-0.15, 0, 0.15] as xOffset (xOffset)}
				<T.Mesh position={[xOffset, 0, 0]} rotation={[0, 0, Math.PI / 2]} castShadow>
					<T.CylinderGeometry args={[0.07, 0.07, 0.05, 16]} />
					<T.MeshStandardMaterial color="#22c55e" roughness={0.8} />
				</T.Mesh>
			{/each}
		</T.Group>

		<!-- Floating Storage & Slot Indicator (Only shown when TAB key or Select button is held) -->
		{#if $showRobotTagsStore || showTagsGamepad}
			<HTML position={[0, 0.75, 0]} center>
				<div
					class={`pointer-events-none flex items-center gap-1.5 rounded border px-2 py-0.5 font-mono text-xs font-bold text-white shadow-lg backdrop-blur-sm select-none ${alliance === 'red' ? 'border-red-500/60 bg-red-950/85' : 'border-blue-500/60 bg-blue-950/85'}`}
				>
					<span>{slotName}</span>
					<span
						class="rounded bg-black/60 px-1.5 text-[10px] font-black text-orange-400 shadow-inner"
					>
						{$robotStorageMap[slotId] ?? 0}
					</span>
					{#if controllerEnabled}
						<span
							class="rounded bg-emerald-500/80 px-1 text-[9px] tracking-tight text-white uppercase"
							>YOU</span
						>
					{:else if isAi}
						<span
							class="rounded bg-amber-500/80 px-1 text-[9px] tracking-tight text-white uppercase"
							>AI</span
						>
					{/if}
				</div>
			</HTML>
		{/if}
	</RigidBody>
</T.Group>
