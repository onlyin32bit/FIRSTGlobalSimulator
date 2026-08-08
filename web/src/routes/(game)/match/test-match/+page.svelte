<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { Canvas, T } from '@threlte/core';
	import { Grid, OrbitControls } from '@threlte/extras';
	import { BasicShadowMap, WebGLRenderer } from 'three';
	import { Button } from '$lib/components/ui/button';
	import { ApiError, api } from '$lib/api';
	import RobotFollowCamera from './RobotFollowCamera.svelte';
	import RobotModel from './RobotModel.svelte';
	import PackField from './PackField.svelte';
	import FieldBoundsDebug from './FieldBoundsDebug.svelte';
	import ScriptedObjects from './ScriptedObjects.svelte';
	import TelemetrySparkline from './TelemetrySparkline.svelte';
	import {
		decodeMatchSnapshot,
		type MatchPhysics as PhysicsModel,
		type MatchPlayer as Player
	} from './match-protocol';
	const activeMatchId = $derived(page.params.matchId ?? '');

	type ObjectFrame = {
		objectId: string;
		positions: Float32Array;
		radius: number;
		color: string;
	};
	type PerformanceWithMemory = Performance & {
		memory?: {
			usedJSHeapSize: number;
			jsHeapSizeLimit: number;
		};
	};
	type NavigatorWithDeviceMemory = Navigator & { deviceMemory?: number };
	let players = $state.raw<Player[]>([]);
	let renderedPlayers = $state.raw<Player[]>([]);
	let objectFrame = $state.raw<ObjectFrame>({
		objectId: 'object',
		positions: new Float32Array(),
		radius: 0.05,
		color: '#f97316'
	});
	let renderedObjectFrame = $state.raw<ObjectFrame>({
		objectId: 'object',
		positions: new Float32Array(),
		radius: 0.05,
		color: '#f97316'
	});
	let physics = $state.raw<PhysicsModel>({
		ballMaterial: 'closed-cell polyurethane foam',
		ballDiameterM: 0.1,
		ballDiameterToleranceM: 0.003,
		ballMassKg: 0.062,
		ballFriction: 0.55,
		ballRestitution: 0.32,
		ballRollingResistanceMps2: 0.4,
		floorMaterial: 'low-pile carpet',
		floorFriction: 0.85,
		robotMassKg: 18,
		robotWidthM: 0.5,
		robotHeightM: 0.5,
		robotLengthM: 0.5,
		robotMaxSpeedMps: 4,
		robotRollingResistanceMps2: 0.35,
		robotMaxAccelerationMps2: 3,
		robotMaxDecelerationMps2: 4,
		robotMaxTurnRateRadps: 2.5,
		robotMaxAngularAccelerationRadps2: 6,
		robotLateralGripMps2: 6,
		robotTractionFriction: 0.85,
		robotTrackWidthM: 0.4,
		ballInertiaFactor: 0.4,
		ballDragCoefficient: 0.47,
		airDensityKgM3: 1.225,
		ballBallFriction: 0.35,
		floorStaticFriction: 0.85,
		floorDynamicFriction: 0.65,
		floorRollingResistanceMps2: 0.55,
		intakeEnabled: false,
		intakeWidthM: 0,
		intakeRadiusM: 0,
		intakeForwardOffsetM: 0,
		intakeCenterHeightM: 0,
		intakeSurfaceSpeedMps: 0,
		rampEnabled: true,
		rampCenterX: -3,
		rampStartZ: -1,
		rampWidthM: 2,
		rampLengthM: 2,
		rampAngleDeg: 15,
		solverPositionIterations: 3,
		solverVelocityIterations: 4,
		maxDepenetrationSpeedMps: 2.5,
		maxBallSpeedMps: 12,
		maxBallAngularSpeedRadps: 240,
		maxDriveForceN: 140,
		maxDrivePowerW: 420,
		maxBrakeForceN: 200,
		storageCapacity: 0,
		intakeRateBps: 0,
		outtakeRateBps: 0,
		outtakeVelocityMps: 0,
		outtakeAngleDeg: 0,
		flywheelWidthM: 0,
		outtakeForwardOffsetM: 0,
		outtakeHeightM: 0
	});
	let robotSpecs = $state({
		capacity: 40,
		intake_rate_bps: 6,
		outtake_rate_bps: 3,
		outtake_velocity_mps: 8,
		outtake_angle_deg: 35,
		flywheel_width_m: 0.35
	});
	let robotSpecsOpen = $state(false);
	let robotSpecsTimer: number | undefined;
	let status = $state('Connecting…');
	let error = $state('');
	let localId = $state('');
	let socket = $state.raw<WebSocket | undefined>(undefined);
	let sequence = 0;
	let pingNonce = 0;
	let pingMs = $state<number | null>(null);
	let packVersion = $state('Loading pack…');
	let fieldAssets = $state<{ visual: string; physics: string; semantics: string } | null>(null);
	let robotAssets = $state<{ visual: string; physics: string } | null>(null);
	type FieldDefinition = {
		colliders: Array<{
			id: string;
			min: [number, number, number];
			max: [number, number, number];
			center?: [number, number, number];
			halfExtents?: [number, number, number];
			axes?: [[number, number, number], [number, number, number], [number, number, number]];
		}>;
		triggers: Array<{ id: string; min: [number, number, number]; max: [number, number, number] }>;
		boundary: { min: [number, number, number]; max: [number, number, number] };
	};
	let fieldDefinition = $state<FieldDefinition | null>(null);
	let fieldDebugOpen = $state(false);
	let robotBoundsDebugOpen = $state(false);
	let semanticEvents = $state<string[]>([]);
	let activeTriggerIds = $derived(
		new Set(
			semanticEvents
				.map((event) => event.match(/^trigger_enter ([^ ]+)/)?.[1])
				.filter((id): id is string => Boolean(id))
		)
	);
	let physicsLoaded = false;
	let snapshotDecodeFailed = false;
	let contacts = $state(0);
	let simulationClock = $state(0);
	let debugOpen = $state(false);
	let gamepadName = $state('No gamepad');
	let gamepadConnected = $state(false);
	let controlSource = $state<'keyboard' | 'gamepad'>('keyboard');
	let inputDrive = $state(0);
	let inputTurn = $state(0);
	let inputIntake = $state(0);
	let inputOuttake = $state(0);
	let cameraMode = $state<'overview' | 'robot'>('overview');
	let cameraDirection = $state<'north' | 'south'>('north');
	let robotCameraDistance = $state(8);
	let clientFps = $state(0);
	let averageFrameMs = $state(0);
	let p95FrameMs = $state(0);
	let maxFrameMs = $state(0);
	let slowFramePercent = $state(0);
	let snapshotRate = $state(0);
	let mainThreadBlockedPercent = $state<number | null>(null);
	let heapUsedMb = $state<number | null>(null);
	let heapLimitMb = $state<number | null>(null);
	let deviceMemoryGb = $state<number | null>(null);
	let logicalCpuCores = $state(0);
	let drawCalls = $state(0);
	let renderedTriangles = $state(0);
	let gpuGeometries = $state(0);
	let gpuTextures = $state(0);
	let serverPhysicsTickMs = $state(0);
	let serverPhysicsLoadPercent = $state(0);
	let serverTick = $state(0);
	let ticksPerSecond = $state(0);
	let targetTicksPerSecond = $state(60);
	let clockDriftMs = $state(0);
	let matchClock = $state(150);
	let matchDurationSeconds = $state(150);
	let preMatchRemainingSeconds = $state(0);
	let matchRunning = $state(false);
	let practiceRunning = $state(false);
	let blueScore = $state(0);
	let redScore = $state(0);
	let globalScore = $state(0);
	let receivedMatchState = false;
	let startCueVisible = $state(false);
	let startCueTimer: number | undefined;
	let integrateMs = $state(0);
	let broadPhaseMs = $state(0);
	let solveMs = $state(0);
	let candidatePairs = $state(0);
	let activeBalls = $state(0);
	let sleepingBalls = $state(0);
	let snapshotBytes = $state(0);
	let serverCpuPercent = $state(0);
	let serverRssMiB = $state(0);
	let socketBufferedBytes = $state(0);
	let viewportSpec = $state('—');
	let pageVisibility = $state<DocumentVisibilityState>('visible');
	let copyState = $state<'COPY' | 'COPIED' | 'FAILED'>('COPY');
	let fpsHistory = $state.raw<number[]>([]);
	let frameP95History = $state.raw<number[]>([]);
	let tpsHistory = $state.raw<number[]>([]);
	let snapshotHistory = $state.raw<number[]>([]);
	let physicsLoadHistory = $state.raw<number[]>([]);
	let pingHistory = $state.raw<number[]>([]);
	let clockDriftHistory = $state.raw<number[]>([]);
	let heapUsagePercent = $derived(
		heapUsedMb !== null && heapLimitMb !== null && heapLimitMb > 0
			? (heapUsedMb / heapLimitMb) * 100
			: null
	);
	let trackedPlayer = $derived(
		renderedPlayers.find((player) => player.id === localId) ?? renderedPlayers[0]
	);
	const pendingPings = new Map<number, number>();
	const pressed = new Set<string>();
	const inputKeys = new Set([
		'w',
		'a',
		's',
		'd',
		'arrowup',
		'arrowdown',
		'arrowleft',
		'arrowright'
	]);
	let renderer: WebGLRenderer | undefined;
	const createRenderer = (canvas: HTMLCanvasElement) => {
		renderer = new WebGLRenderer({ canvas, antialias: false, powerPreference: 'high-performance' });
		// One low-cost shadow map is enough to anchor the robots and balls to
		// the field. BasicShadowMap avoids the extra filtering passes of PCF.
		renderer.shadowMap.enabled = true;
		renderer.shadowMap.type = BasicShadowMap;
		return renderer;
	};
	let lastSentDrive = Number.NaN;
	let lastSentTurn = Number.NaN;
	let lastSentIntake = Number.NaN;
	let lastSentOuttake = Number.NaN;
	let lastInputSentAt = 0;
	const highIsBadTone = (value: number, warning: number, critical: number) =>
		value >= critical ? 'text-fuchsia-300' : value >= warning ? 'text-amber-300' : 'text-cyan-300';
	const lowIsBadTone = (value: number, warning: number, critical: number) =>
		value < critical ? 'text-fuchsia-300' : value < warning ? 'text-amber-300' : 'text-cyan-300';
	const highIsBadMarker = (value: number, warning: number, critical: number) =>
		value >= critical ? '×' : value >= warning ? '!' : '·';
	const lowIsBadMarker = (value: number, warning: number, critical: number) =>
		value < critical ? '×' : value < warning ? '!' : '·';
	const formatMatchClock = (seconds: number) => {
		const wholeSeconds = Math.max(0, Math.ceil(seconds - 0.001));
		return `${Math.floor(wholeSeconds / 60)}:${String(wholeSeconds % 60).padStart(2, '0')}`;
	};
	const redRoster = $derived(players.filter((player) => player.teamName === 'red').slice(0, 3));
	const blueRoster = $derived(players.filter((player) => player.teamName === 'blue').slice(0, 3));
	const countdownVisible = $derived(!matchRunning && preMatchRemainingSeconds > 0.05);

	function diagnosticsReport() {
		return {
			schema: 'first-global-simulator.match-debug.v2',
			capturedAt: new Date().toISOString(),
			thresholds: {
				fps: { normalMin: 55, criticalBelow: 30 },
				frameP95Ms: { normalBelow: 20, criticalAt: 40 },
				snapshotsPerSecond: { target: 20, normalMin: 18, criticalBelow: 10 },
				tps: { target: targetTicksPerSecond, normalMin: 58, criticalBelow: 50 },
				physicsLoadPercent: { normalBelow: 50, criticalAt: 80 },
				pingMs: { normalBelow: 100, criticalAt: 250 },
				simulationLagMs: { normalAbsoluteBelow: 100, criticalAbsoluteAt: 500 }
			},
			connection: {
				status,
				webSocketState: socket?.readyState ?? WebSocket.CLOSED,
				bufferedBytes: socketBufferedBytes,
				pingMs,
				snapshotsPerSecond: snapshotRate
			},
			client: {
				fps: clientFps,
				frameMs: { average: averageFrameMs, p95: p95FrameMs, max: maxFrameMs },
				slowFramePercent,
				mainThreadBlockedPercent,
				heapMiB: { used: heapUsedMb, limit: heapLimitMb, usagePercent: heapUsagePercent }
			},
			server: {
				tick: serverTick,
				ticksPerSecond,
				targetTicksPerSecond,
				physicsTickMs: serverPhysicsTickMs,
				physicsLoadPercent: serverPhysicsLoadPercent,
				simulationClockSeconds: simulationClock,
				matchClockSeconds: matchClock,
				matchDurationSeconds,
				preMatchRemainingSeconds,
				matchRunning,
				clockDriftMs,
				stagesMs: { integrate: integrateMs, broadPhase: broadPhaseMs, solve: solveMs },
				candidatePairs,
				activeBalls,
				sleepingBalls,
				snapshotBytes,
				process: { cpuPercent: serverCpuPercent, rssMiB: serverRssMiB }
			},
			renderer: {
				drawCalls,
				triangles: renderedTriangles,
				geometries: gpuGeometries,
				textures: gpuTextures
			},
			scene: {
				players: players.length,
				objects: objectFrame.positions.length / 3,
				contacts,
				object: { id: objectFrame.objectId, radius: objectFrame.radius, color: objectFrame.color }
			},
			device: {
				logicalCpuThreads: logicalCpuCores,
				memoryGiB: deviceMemoryGb,
				viewport: viewportSpec,
				visibility: pageVisibility,
				userAgent: navigator.userAgent
			},
			controls: {
				source: controlSource,
				drive: inputDrive,
				turn: inputTurn,
				intake: inputIntake,
				gamepad: gamepadConnected ? gamepadName : null,
				camera: `${cameraMode}${cameraMode === 'robot' ? `/${cameraDirection}/${robotCameraDistance.toFixed(1)}m` : ''}`
			},
			packVersion,
			physics,
			players: players.map((player) => ({
				...player,
				velocityX: player.velocityX ?? 0,
				velocityY: player.velocityY ?? 0,
				velocityZ: player.velocityZ ?? 0
			})),
			history60s: {
				sampleIntervalSeconds: 1,
				order: 'oldest-to-newest',
				fps: fpsHistory,
				frameP95Ms: frameP95History,
				tps: tpsHistory,
				snapshotsPerSecond: snapshotHistory,
				physicsLoadPercent: physicsLoadHistory,
				pingMs: pingHistory,
				absoluteSimulationLagMs: clockDriftHistory
			}
		};
	}

	async function copyDiagnostics() {
		const text = JSON.stringify(diagnosticsReport(), null, 2);
		try {
			await navigator.clipboard.writeText(text);
			copyState = 'COPIED';
		} catch {
			copyState = 'FAILED';
		}
		window.setTimeout(() => (copyState = 'COPY'), 1500);
	}

	function applyDeadzone(value: number, deadzone = 0.12) {
		const magnitude = Math.abs(value);
		if (magnitude <= deadzone) return 0;
		return Math.sign(value) * ((magnitude - deadzone) / (1 - deadzone));
	}

	function activeGamepad() {
		if (!navigator.getGamepads) return null;
		return Array.from(navigator.getGamepads()).find((gamepad) => gamepad?.connected) ?? null;
	}

	function refreshGamepadStatus() {
		const gamepad = activeGamepad();
		gamepadConnected = gamepad !== null;
		gamepadName = gamepad?.id ?? 'No gamepad';
	}

	function sampleInput() {
		const keyboardTurn =
			Number(pressed.has('d') || pressed.has('arrowright')) -
			Number(pressed.has('a') || pressed.has('arrowleft'));
		const keyboardDrive =
			Number(pressed.has('w') || pressed.has('arrowup')) -
			Number(pressed.has('s') || pressed.has('arrowdown'));

		const gamepad = activeGamepad();
		let gamepadDrive = 0;
		let gamepadTurn = 0;
		if (gamepad) {
			gamepadDrive = applyDeadzone(-(gamepad.axes[1] ?? 0));
			gamepadTurn = applyDeadzone(gamepad.axes[2] ?? gamepad.axes[0] ?? 0);
			// The A button commands neutral input, allowing the server-side
			// brake limit to act.
			if (gamepad.buttons[0]?.pressed) {
				gamepadDrive = 0;
				gamepadTurn = 0;
			}
		}

		const keyboardActive = keyboardDrive !== 0 || keyboardTurn !== 0;
		return {
			drive: keyboardActive ? keyboardDrive : gamepadDrive,
			turn: keyboardActive ? keyboardTurn : gamepadTurn,
			intake: 0,
			outtake: 0,
			source: (keyboardActive ? 'keyboard' : gamepad ? 'gamepad' : 'keyboard') as
				'keyboard' | 'gamepad'
		};
	}

	function sendInputFrom(input: ReturnType<typeof sampleInput>, force = false) {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		const now = performance.now();
		const changed =
			Math.abs(input.drive - lastSentDrive) > 0.005 ||
			Math.abs(input.turn - lastSentTurn) > 0.005 ||
			Math.abs(input.intake - lastSentIntake) > 0.005 ||
			Math.abs(input.outtake - lastSentOuttake) > 0.005;
		if (!force && !changed && now - lastInputSentAt < 250) return;

		controlSource = input.source;
		inputDrive = input.drive;
		inputTurn = input.turn;
		inputIntake = input.intake;
		inputOuttake = input.outtake;
		lastSentDrive = input.drive;
		lastSentTurn = input.turn;
		lastSentIntake = input.intake;
		lastSentOuttake = input.outtake;
		lastInputSentAt = now;
		socket.send(
			JSON.stringify({
				type: 'input',
				sequence: ++sequence,
				move_x: input.turn,
				move_z: input.drive,
				intake_power: input.intake,
				outtake_power: input.outtake
			})
		);
	}
	function sendInput(force = false) {
		sendInputFrom(sampleInput(), force);
	}
	function sendPing() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		const nonce = ++pingNonce;
		pendingPings.set(nonce, performance.now());
		socket.send(JSON.stringify({ type: 'ping', nonce }));
	}
	function continuePractice() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		socket.send(JSON.stringify({ type: 'continue_practice' }));
	}
	function endPractice() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		socket.send(JSON.stringify({ type: 'end_practice' }));
	}
	function sendRobotSpecs() {
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		socket.send(
			JSON.stringify({
				type: 'robot_specs',
				capacity: robotSpecs.capacity,
				intake_rate_bps: robotSpecs.intake_rate_bps,
				outtake_rate_bps: robotSpecs.outtake_rate_bps,
				outtake_velocity_mps: robotSpecs.outtake_velocity_mps,
				outtake_angle_deg: robotSpecs.outtake_angle_deg,
				flywheel_width_m: robotSpecs.flywheel_width_m
			})
		);
	}
	function scheduleRobotSpecs() {
		if (robotSpecsTimer !== undefined) window.clearTimeout(robotSpecsTimer);
		robotSpecsTimer = window.setTimeout(() => {
			robotSpecsTimer = undefined;
			sendRobotSpecs();
		}, 180);
	}

	onMount(() => {
		let disposed = false;
		let animationFrame = 0;
		let previousFrameTime = performance.now();
		let statsWindowStartedAt = previousFrameTime;
		let framesInWindow = 0;
		let stateMessagesInWindow = 0;
		let longTaskTimeMs = 0;
		let frameSamples: number[] = [];
		logicalCpuCores = navigator.hardwareConcurrency || 0;
		deviceMemoryGb = (navigator as NavigatorWithDeviceMemory).deviceMemory ?? null;
		viewportSpec = `${window.innerWidth}×${window.innerHeight}@${window.devicePixelRatio.toFixed(2)}x`;
		pageVisibility = document.visibilityState;
		let longTaskObserver: PerformanceObserver | undefined;
		if (PerformanceObserver.supportedEntryTypes.includes('longtask')) {
			longTaskObserver = new PerformanceObserver((list) => {
				for (const entry of list.getEntries()) longTaskTimeMs += entry.duration;
			});
			longTaskObserver.observe({ type: 'longtask', buffered: false });
		}
		const interpolateScene = (frameTime: number) => {
			const frameDurationMs = frameTime - previousFrameTime;
			const dt = Math.min(frameDurationMs / 1000, 0.05);
			previousFrameTime = frameTime;
			framesInWindow += 1;
			frameSamples.push(frameDurationMs);
			const statsElapsedMs = frameTime - statsWindowStartedAt;
			if (statsElapsedMs >= 1000) {
				clientFps = (framesInWindow * 1000) / statsElapsedMs;
				averageFrameMs = statsElapsedMs / Math.max(framesInWindow, 1);
				const sortedFrames = frameSamples.toSorted((a, b) => a - b);
				p95FrameMs = sortedFrames[Math.floor((sortedFrames.length - 1) * 0.95)] ?? 0;
				maxFrameMs = sortedFrames.at(-1) ?? 0;
				slowFramePercent =
					(frameSamples.filter((sample) => sample >= 20).length * 100) /
					Math.max(frameSamples.length, 1);
				snapshotRate = (stateMessagesInWindow * 1000) / statsElapsedMs;
				mainThreadBlockedPercent = longTaskObserver
					? Math.min(100, (longTaskTimeMs * 100) / statsElapsedMs)
					: null;

				const memory = (performance as PerformanceWithMemory).memory;
				heapUsedMb = memory ? memory.usedJSHeapSize / 1_048_576 : null;
				heapLimitMb = memory ? memory.jsHeapSizeLimit / 1_048_576 : null;
				if (renderer) {
					drawCalls = renderer.info.render.calls;
					renderedTriangles = renderer.info.render.triangles;
					gpuGeometries = renderer.info.memory.geometries;
					gpuTextures = renderer.info.memory.textures;
				}
				socketBufferedBytes = socket?.bufferedAmount ?? 0;
				viewportSpec = `${window.innerWidth}×${window.innerHeight}@${window.devicePixelRatio.toFixed(2)}x`;
				pageVisibility = document.visibilityState;
				fpsHistory = [...fpsHistory.slice(-59), clientFps];
				frameP95History = [...frameP95History.slice(-59), p95FrameMs];
				tpsHistory = [...tpsHistory.slice(-59), ticksPerSecond];
				snapshotHistory = [...snapshotHistory.slice(-59), snapshotRate];
				physicsLoadHistory = [...physicsLoadHistory.slice(-59), serverPhysicsLoadPercent];
				pingHistory = [...pingHistory.slice(-59), pingMs ?? pingHistory.at(-1) ?? 0];
				clockDriftHistory = [...clockDriftHistory.slice(-59), Math.abs(clockDriftMs)];

				statsWindowStartedAt = frameTime;
				framesInWindow = 0;
				stateMessagesInWindow = 0;
				longTaskTimeMs = 0;
				frameSamples = [];
			}
			// Gamepad is sampled every animation frame so input reaches the
			// network at frame rate instead of the 20 Hz input timer. State writes
			// are guarded to avoid reactivity churn.
			const input = sampleInput();
			if (input.drive !== inputDrive) inputDrive = input.drive;
			if (input.turn !== inputTurn) inputTurn = input.turn;
			if (input.intake !== inputIntake) inputIntake = input.intake;
			if (input.outtake !== inputOuttake) inputOuttake = input.outtake;
			if (input.source !== controlSource) controlSource = input.source;
			sendInputFrom(input);

			const currentById = new Map(renderedPlayers.map((player) => [player.id, player]));
			const blend = 1 - Math.exp(-24 * dt);

			if (players.length === 0) {
				if (renderedPlayers.length !== 0) renderedPlayers = [];
			} else {
				let changed = players.length !== renderedPlayers.length;
				const nextPlayers = players.map((target) => {
					const current = currentById.get(target.id);
					if (!current) {
						changed = true;
						return { ...target };
					}

					const distance = Math.hypot(
						target.x - current.x,
						target.y - current.y,
						target.z - current.z
					);
					const yawDelta = Math.atan2(
						Math.sin(target.yaw - current.yaw),
						Math.cos(target.yaw - current.yaw)
					);
					if (distance < 0.0001 && Math.abs(yawDelta) < 0.0001) return current;

					changed = true;
					if (distance > 2) return { ...target };

					// Interpolate the shortest rotation path across the ±π boundary.
					return {
						...target,
						x: current.x + (target.x - current.x) * blend,
						y: current.y + (target.y - current.y) * blend,
						z: current.z + (target.z - current.z) * blend,
						yaw: current.yaw + yawDelta * blend
					};
				});
				if (changed) renderedPlayers = nextPlayers;
			}

			const targetPositions = objectFrame.positions;
			const renderedPositions = renderedObjectFrame.positions;
			if (targetPositions.length !== renderedPositions.length) {
				// Allocate only when the pack changes its object count. Normal
				// animation reuses these tuples to avoid 500 allocations per frame.
				renderedObjectFrame = {
					...objectFrame,
					positions: new Float32Array(targetPositions)
				};
			} else {
				let changed =
					objectFrame.objectId !== renderedObjectFrame.objectId ||
					objectFrame.radius !== renderedObjectFrame.radius ||
					objectFrame.color !== renderedObjectFrame.color;
				for (let index = 0; index < targetPositions.length; index += 3) {
					const dx = targetPositions[index] - renderedPositions[index];
					const dy = targetPositions[index + 1] - renderedPositions[index + 1];
					const dz = targetPositions[index + 2] - renderedPositions[index + 2];
					const distance = Math.hypot(dx, dy, dz);
					if (distance < 0.0001) continue;

					changed = true;
					if (distance > 0.75) {
						renderedPositions[index] = targetPositions[index];
						renderedPositions[index + 1] = targetPositions[index + 1];
						renderedPositions[index + 2] = targetPositions[index + 2];
					} else {
						renderedPositions[index] += dx * blend;
						renderedPositions[index + 1] += dy * blend;
						renderedPositions[index + 2] += dz * blend;
					}
				}
				if (changed) {
					renderedObjectFrame = { ...objectFrame, positions: renderedPositions };
				}
			}

			animationFrame = window.requestAnimationFrame(interpolateScene);
		};
		animationFrame = window.requestAnimationFrame(interpolateScene);
		const gamepadChanged = () => refreshGamepadStatus();
		const keydown = (event: KeyboardEvent) => {
			if (event.ctrlKey && event.code === 'F3') {
				debugOpen = !debugOpen;
				event.preventDefault();
				return;
			}
			if (!event.repeat && event.key.toLowerCase() === 'c') {
				cameraMode = cameraMode === 'overview' ? 'robot' : 'overview';
				event.preventDefault();
				return;
			}
			if (!event.repeat && event.key.toLowerCase() === 'f' && cameraMode === 'robot') {
				cameraDirection = cameraDirection === 'north' ? 'south' : 'north';
				event.preventDefault();
				return;
			}
			if (!event.repeat && event.key.toLowerCase() === 'b') {
				fieldDebugOpen = !fieldDebugOpen;
				event.preventDefault();
				return;
			}
			const key = event.key.toLowerCase();
			if (!inputKeys.has(key)) return;
			pressed.add(key);
			event.preventDefault();
			sendInput();
		};
		const keyup = (event: KeyboardEvent) => {
			const key = event.key.toLowerCase();
			if (!inputKeys.has(key)) return;
			pressed.delete(key);
			event.preventDefault();
			sendInput();
		};
		window.addEventListener('keydown', keydown);
		window.addEventListener('keyup', keyup);
		window.addEventListener('gamepadconnected', gamepadChanged);
		window.addEventListener('gamepaddisconnected', gamepadChanged);
		refreshGamepadStatus();
		const inputTimer = window.setInterval(sendInput, 50);
		const pingTimer = window.setInterval(sendPing, 2000);

		const connect = async () => {
			if (!activeMatchId) {
				error = 'A persisted match ID is required. Create or join a match from the dashboard.';
				status = 'Unavailable';
				return;
			}
			try {
				const [ticket, currentUser, assets, metadata, starterBotAssets] = await Promise.all([
					api.createMatchTicket(activeMatchId),
					api.getCurrentUser(),
					api.getGamePackAssets('fgc-2026'),
					api.getGamePackMetadata('fgc-2026'),
					api.getRobotAssets('StarterBot')
				]);
				if (disposed) return;

				localId = currentUser.user.id;
				fieldAssets = assets;
				robotAssets = starterBotAssets;
				fieldDefinition = metadata.fieldDefinition;
				const nextSocket = new WebSocket(ticket.ws_url);
				nextSocket.binaryType = 'arraybuffer';
				socket = nextSocket;
				nextSocket.onopen = () => {
					if (disposed) return;
					status = 'Connected';
					error = '';
					sendPing();
					sendInput(true);
				};
				nextSocket.onclose = (event) => {
					if (disposed) return;
					status = 'Disconnected';
					if (event.code !== 1000) {
						error = `Match server closed the connection (code ${event.code}).`;
					}
				};
				nextSocket.onerror = () => {
					if (!disposed)
						error = 'Unable to reach ws://localhost:3000. Start the Rust match server.';
				};
				nextSocket.onmessage = (event) => {
					if (disposed) return;
					try {
						if (event.data instanceof ArrayBuffer) {
							const message = decodeMatchSnapshot(event.data);
							if (snapshotDecodeFailed) {
								snapshotDecodeFailed = false;
								status = 'Connected';
								error = '';
							}
							stateMessagesInWindow += 1;
							snapshotBytes = event.data.byteLength;
							players = message.players;
							objectFrame = {
								objectId: message.objectId,
								positions: message.positions,
								radius: message.objectRadius,
								color: message.objectColor
							};
							contacts = message.contacts;
							if (message.matchRunning && receivedMatchState && !matchRunning) {
								startCueVisible = true;
								if (startCueTimer !== undefined) window.clearTimeout(startCueTimer);
								startCueTimer = window.setTimeout(() => (startCueVisible = false), 900);
							}
							matchClock = message.matchClock;
							matchDurationSeconds = message.matchDurationSeconds;
							preMatchRemainingSeconds = message.preMatchRemainingSeconds;
							matchRunning = message.matchRunning;
							practiceRunning = message.practiceRunning;
							blueScore = message.score.blue;
							redScore = message.score.red;
							globalScore = message.score.global;
							receivedMatchState = true;
							simulationClock = message.simulationClock;
							serverTick = message.tick;
							serverPhysicsTickMs = message.physicsTickMs;
							serverPhysicsLoadPercent = message.physicsLoadPercent;
							ticksPerSecond = message.ticksPerSecond;
							targetTicksPerSecond = message.targetTicksPerSecond;
							clockDriftMs = message.clockDriftMs;
							integrateMs = message.integrateMs;
							broadPhaseMs = message.broadPhaseMs;
							solveMs = message.solveMs;
							candidatePairs = message.candidatePairs;
							activeBalls = message.activeBalls;
							sleepingBalls = message.sleepingBalls;
							serverCpuPercent = message.serverCpuPercent;
							serverRssMiB = message.serverRssMiB;
							if (message.physics && !physicsLoaded) {
								physics = message.physics;
								physicsLoaded = true;
								robotSpecs.capacity = Math.round(physics.storageCapacity);
								robotSpecs.intake_rate_bps = physics.intakeRateBps;
								robotSpecs.outtake_rate_bps = physics.outtakeRateBps;
								robotSpecs.outtake_velocity_mps = physics.outtakeVelocityMps;
								robotSpecs.outtake_angle_deg = physics.outtakeAngleDeg;
								robotSpecs.flywheel_width_m = physics.flywheelWidthM;
							}
							packVersion = `${message.gamePackId} · v${message.gamePackVersion}`;
							semanticEvents = message.semanticEvents;
							return;
						}
						const message = JSON.parse(event.data);
						if (message.type === 'pong') {
							const started = pendingPings.get(message.nonce);
							if (started !== undefined) {
								const sample = performance.now() - started;
								pingMs = pingMs === null ? sample : pingMs * 0.7 + sample * 0.3;
								pendingPings.delete(message.nonce);
							}
						}
					} catch (cause) {
						if (!snapshotDecodeFailed) {
							console.error('[match] Failed to decode server snapshot', cause);
						}
						snapshotDecodeFailed = true;
						status = 'Protocol error';
						error = `Server snapshot rejected: ${cause instanceof Error ? cause.message : 'unknown format'}`;
					}
				};
			} catch (e) {
				if (disposed) return;
				error = e instanceof ApiError ? e.message : 'Unable to join the live test match.';
				status = 'Unavailable';
			}
		};
		void connect();

		return () => {
			disposed = true;
			window.cancelAnimationFrame(animationFrame);
			longTaskObserver?.disconnect();
			window.clearInterval(inputTimer);
			window.clearInterval(pingTimer);
			if (startCueTimer !== undefined) window.clearTimeout(startCueTimer);
			if (robotSpecsTimer !== undefined) window.clearTimeout(robotSpecsTimer);
			window.removeEventListener('keydown', keydown);
			window.removeEventListener('keyup', keyup);
			window.removeEventListener('gamepadconnected', gamepadChanged);
			window.removeEventListener('gamepaddisconnected', gamepadChanged);
			socket?.close();
			socket = undefined;
			pendingPings.clear();
		};
	});
</script>

<div class="relative h-[calc(100vh-3.5rem)] overflow-hidden bg-slate-950">
	<!-- Compact field scoreboard. SU containment and EXT extinguishing scores
	     arrive in the SCORE protocol section and are summed live. -->
	<section
		class="pointer-events-none fixed bottom-0 left-1/2 z-20 w-[600px] -translate-x-1/2 rounded-t-2xl bg-black font-sans leading-none font-black text-white drop-shadow-[0_3px_5px_rgba(0,0,0,0.65)]"
		aria-label="Match scoreboard"
	>
		<div
			class="m-2 flex items-stretch overflow-hidden rounded-lg border-2 border-slate-950 bg-slate-950 shadow-2xl"
		>
			<div
				class="flex min-h-[132px] flex-1 flex-col justify-center gap-2 bg-[#c82d31] px-4 text-right text-[clamp(1rem,2.3vw,1.65rem)]"
			>
				{#each Array(3) as _, index (index)}
					<p class="truncate">{redRoster[index]?.name ?? '—'}</p>
				{/each}
			</div>
			<div class="flex w-[clamp(152px,23vw,206px)] shrink-0 flex-col bg-slate-950 text-center">
				<div
					class="flex flex-1 items-center justify-center bg-[#f8bd4c] px-2 text-[clamp(2.65rem,6.2vw,5.3rem)] tracking-[-0.08em] text-slate-950 tabular-nums"
				>
					{formatMatchClock(matchClock)}
				</div>
				<div
					class="grid h-14 grid-cols-2 border-t-2 border-slate-950 text-[clamp(2rem,4.2vw,3.7rem)] tabular-nums sm:h-20"
				>
					<div class="flex items-center justify-center bg-[#c82d31]">{redScore}</div>
					<div class="flex items-center justify-center border-l-2 border-slate-950 bg-[#627fe9]">
						{blueScore}
					</div>
				</div>
				{#if globalScore > 0}
					<div
						class="flex items-center justify-center gap-1 border-t border-slate-700 bg-slate-900 py-0.5 text-[0.9rem] text-[#d7ff7b]"
					>
						EXT {globalScore}
					</div>
				{/if}
			</div>
			<div
				class="flex min-h-[132px] flex-1 flex-col justify-center gap-2 bg-[#627fe9] px-4 text-[clamp(1rem,2.3vw,1.65rem)]"
			>
				{#each Array(3) as _, index (index)}
					<p class="truncate">{blueRoster[index]?.name ?? '—'}</p>
				{/each}
			</div>
		</div>
		<div
			class="grid grid-cols-2 overflow-hidden bg-[#d7ff7b] px-3 py-1 text-[0.6rem] tracking-normal text-slate-950 sm:text-xs"
		>
			<span class="text-right">FIRST Global 2026</span>
		</div>
	</section>

	{#if !matchRunning && practiceRunning}
		<div class="fixed bottom-40 left-1/2 z-20 -translate-x-1/2">
			<Button
				variant="outline"
				class="border-[#d7ff7b]/60 bg-black/70 text-[#d7ff7b] hover:bg-[#d7ff7b]/10"
				onclick={endPractice}
			>
				End practice (freeze field)
			</Button>
		</div>
	{/if}
	{#if !matchRunning && !practiceRunning && receivedMatchState}
		<div class="fixed bottom-40 left-1/2 z-20 -translate-x-1/2">
			<Button
				variant="outline"
				class="border-white/20 bg-black/70 text-white hover:bg-white/10"
				onclick={continuePractice}
			>
				Continue practising past the timer
			</Button>
		</div>
	{/if}

	{#if countdownVisible || startCueVisible}
		<div
			class="pointer-events-none absolute inset-0 z-30 grid place-items-center"
			aria-live="assertive"
		>
			<p
				class="font-black tracking-[-0.08em] text-white drop-shadow-[0_7px_16px_rgba(0,0,0,0.9)] select-none {startCueVisible
					? 'text-[clamp(4.5rem,16vw,13rem)] tracking-[-0.1em]'
					: 'text-[clamp(7rem,24vw,18rem)]'}"
			>
				{startCueVisible ? 'START' : Math.max(1, Math.ceil(preMatchRemainingSeconds))}
			</p>
		</div>
	{/if}

	<div
		class="absolute top-4 left-4 z-10 rounded-lg border border-white/15 bg-black/60 px-4 py-3 text-sm text-white backdrop-blur"
	>
		<p class="font-semibold">Live match</p>
		<p class="mt-1 text-white/70">
			{status} · {players.length} player{players.length === 1 ? '' : 's'} · Ping: {pingMs === null
				? '—'
				: `${Math.round(pingMs)} ms`}
		</p>
		<p class="mt-1 text-xs text-white/60">Pack: {packVersion}</p>
		<p class="mt-2 text-xs text-white/60">W/S drive · A/D turn · Gamepad: LS-Y + RS-X</p>
		<p class="mt-1 max-w-72 truncate text-xs text-white/50" title={gamepadName}>
			<span class={gamepadConnected ? 'text-cyan-300' : 'text-white/35'}>●</span>
			{gamepadConnected ? gamepadName : 'Connect a gamepad and press a button'}
		</p>
		<p class="mt-1 text-xs text-white/40">A: brake</p>
		<p class="mt-1 text-xs text-white/40">
			C: camera · F: flip north/south · B: field bounds · Ctrl+F3: diagnostics
		</p>
		{#if error}<p class="mt-2 text-fuchsia-300">✖ {error}</p>{/if}
	</div>
	<div class="absolute top-4 right-4 z-10 flex items-center gap-2">
		<Button
			variant="outline"
			class="border-white/20 bg-black/40 text-white hover:bg-white/10"
			onclick={() => (cameraMode = cameraMode === 'overview' ? 'robot' : 'overview')}
		>
			{cameraMode === 'robot' ? 'Overview camera' : 'Follow robot'}
		</Button>
		{#if cameraMode === 'robot'}
			<Button
				variant="outline"
				class="border-amber-300/30 bg-black/40 text-amber-100 hover:bg-amber-300/10"
				onclick={() => (cameraDirection = cameraDirection === 'north' ? 'south' : 'north')}
			>
				Flip to {cameraDirection === 'north' ? 'south' : 'north'}
			</Button>
			<label
				class="flex h-10 items-center gap-2 rounded-md border border-white/20 bg-black/40 px-3 text-xs text-white/75"
				for="robot-camera-distance"
			>
				<span class="whitespace-nowrap">Zoom {robotCameraDistance.toFixed(1)} m</span>
				<input
					id="robot-camera-distance"
					type="range"
					min="1"
					max="18"
					step="0.5"
					bind:value={robotCameraDistance}
					class="w-24 accent-cyan-300"
					aria-label="Robot follow camera distance"
				/>
			</label>
		{/if}
		<Button
			variant="outline"
			class="border-cyan-300/30 bg-black/40 text-cyan-100 hover:bg-cyan-300/10"
			onclick={() => (debugOpen = !debugOpen)}
		>
			{debugOpen ? 'Hide diagnostics' : 'Diagnostics'}
		</Button>
		<Button
			variant="outline"
			class={fieldDebugOpen
				? 'border-emerald-300/60 bg-emerald-300/15 text-emerald-100 hover:bg-emerald-300/25'
				: 'border-sky-300/30 bg-black/40 text-sky-100 hover:bg-sky-300/10'}
			onclick={() => (fieldDebugOpen = !fieldDebugOpen)}
		>
			{fieldDebugOpen ? 'Hide field bounds' : 'Field bounds'}
		</Button>
		<Button
			variant="outline"
			class={robotBoundsDebugOpen
				? 'border-red-300/60 bg-red-300/15 text-red-100 hover:bg-red-300/25'
				: 'border-red-300/30 bg-black/40 text-red-100 hover:bg-red-300/10'}
			onclick={() => (robotBoundsDebugOpen = !robotBoundsDebugOpen)}
		>
			{robotBoundsDebugOpen ? 'Hide robot physics' : 'Robot physics'}
		</Button>
		<Button
			href="/dashboard"
			variant="outline"
			class="border-white/20 bg-black/40 text-white hover:bg-white/10">Leave match</Button
		>
	</div>
	{#if robotSpecsOpen}
		<section
			class="absolute top-18 right-4 z-10 w-[19rem] rounded-lg border border-white/20 bg-black/85 p-3 text-sm text-white backdrop-blur"
			aria-label="Robot mechanics specs"
		>
			<div class="mb-2 flex items-baseline justify-between">
				<h2 class="font-semibold">ROBOT MECH SPECS</h2>
				<span class="text-xs text-white/40">apply live</span>
			</div>
			<p class="mb-2 text-xs leading-relaxed text-white/55">
				Space/E intake, E/LB outtake the wide flywheel. Values rebalance your robot without
				restarting the match.
			</p>
			<div class="space-y-2 text-xs">
				<label class="flex items-center justify-between gap-2" for="spec-capacity">
					<span class="text-white/70">Storage</span>
					<input
						id="spec-capacity"
						type="range"
						min="1"
						max="80"
						step="1"
						bind:value={robotSpecs.capacity}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums">{robotSpecs.capacity}</span>
				</label>
				<label class="flex items-center justify-between gap-2" for="spec-intake">
					<span class="text-white/70">Intake rate</span>
					<input
						id="spec-intake"
						type="range"
						min="1"
						max="20"
						step="0.5"
						bind:value={robotSpecs.intake_rate_bps}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums">{robotSpecs.intake_rate_bps.toFixed(1)}/s</span
					>
				</label>
				<label class="flex items-center justify-between gap-2" for="spec-outrate">
					<span class="text-white/70">Outtake rate</span>
					<input
						id="spec-outrate"
						type="range"
						min="1"
						max="20"
						step="0.5"
						bind:value={robotSpecs.outtake_rate_bps}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums"
						>{robotSpecs.outtake_rate_bps.toFixed(1)}/s</span
					>
				</label>
				<label class="flex items-center justify-between gap-2" for="spec-vel">
					<span class="text-white/70">Launch speed</span>
					<input
						id="spec-vel"
						type="range"
						min="2"
						max="12"
						step="0.5"
						bind:value={robotSpecs.outtake_velocity_mps}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums"
						>{robotSpecs.outtake_velocity_mps.toFixed(1)} m/s</span
					>
				</label>
				<label class="flex items-center justify-between gap-2" for="spec-angle">
					<span class="text-white/70">Launch angle</span>
					<input
						id="spec-angle"
						type="range"
						min="5"
						max="60"
						step="1"
						bind:value={robotSpecs.outtake_angle_deg}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums">{robotSpecs.outtake_angle_deg}°</span>
				</label>
				<label class="flex items-center justify-between gap-2" for="spec-width">
					<span class="text-white/70">Flywheel width</span>
					<input
						id="spec-width"
						type="range"
						min="0.1"
						max="0.6"
						step="0.05"
						bind:value={robotSpecs.flywheel_width_m}
						onchange={scheduleRobotSpecs}
						class="w-32 accent-amber-300"
					/>
					<span class="w-10 text-right tabular-nums"
						>{robotSpecs.flywheel_width_m.toFixed(2)} m</span
					>
				</label>
			</div>
		</section>
	{/if}
	{#if debugOpen}
		<aside
			class="absolute top-18 right-4 z-10 max-h-[calc(100vh-5.5rem)] w-[19rem] overflow-y-auto border border-white/20 bg-black/92 p-2 font-mono text-[10px] leading-tight text-slate-200"
			aria-label="Match diagnostics"
		>
			<div class="flex items-center justify-between border-b border-white/15 pb-1">
				<p class="font-semibold text-white">MATCH.DEBUG</p>
				<div class="flex items-center gap-2">
					<span class="text-white/45">60s · ^F3</span>
					<button
						type="button"
						class="border border-white/25 px-1.5 py-0.5 text-white/70 hover:border-cyan-300 hover:text-cyan-300 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-cyan-300"
						onclick={copyDiagnostics}
						aria-label="Copy all match diagnostics and physics specifications">{copyState}</button
					>
				</div>
			</div>
			<p class="flex items-center justify-between border-b border-white/8 py-1 text-white/45">
				<span>· OK&nbsp;&nbsp;! WARN&nbsp;&nbsp;× CRIT</span>
				<span class={socket?.readyState === 1 ? 'text-cyan-300' : 'text-fuchsia-300'}>
					NET {socket?.readyState === 1 ? 'OPEN' : socket?.readyState === 0 ? 'WAIT' : 'DOWN'}
				</span>
			</p>
			<div aria-label="Performance history, newest value at right">
				<TelemetrySparkline
					label={`${lowIsBadMarker(clientFps, 55, 30)} FPS`}
					value={clientFps.toFixed(0)}
					unit="fps"
					samples={fpsHistory}
					max={60}
					reference={55}
					tone={lowIsBadTone(clientFps, 55, 30)}
				/>
				<TelemetrySparkline
					label={`${highIsBadMarker(p95FrameMs, 20, 40)} FRM`}
					value={p95FrameMs.toFixed(1)}
					unit="ms p95"
					samples={frameP95History}
					max={50}
					reference={20}
					tone={highIsBadTone(p95FrameMs, 20, 40)}
				/>
				<TelemetrySparkline
					label={`${lowIsBadMarker(ticksPerSecond, 58, 50)} TPS`}
					value={ticksPerSecond.toFixed(1)}
					unit={`/${targetTicksPerSecond.toFixed(0)}`}
					samples={tpsHistory}
					max={60}
					reference={58}
					tone={lowIsBadTone(ticksPerSecond, 58, 50)}
				/>
				<TelemetrySparkline
					label={`${lowIsBadMarker(snapshotRate, 18, 10)} NET`}
					value={snapshotRate.toFixed(1)}
					unit="Hz"
					samples={snapshotHistory}
					max={20}
					reference={18}
					tone={lowIsBadTone(snapshotRate, 18, 10)}
				/>
				<TelemetrySparkline
					label={`${highIsBadMarker(serverPhysicsLoadPercent, 50, 80)} PHY`}
					value={serverPhysicsLoadPercent.toFixed(0)}
					unit="%"
					samples={physicsLoadHistory}
					max={100}
					reference={50}
					tone={highIsBadTone(serverPhysicsLoadPercent, 50, 80)}
				/>
				<TelemetrySparkline
					label={`${pingMs === null ? '?' : highIsBadMarker(pingMs, 100, 250)} RTT`}
					value={pingMs === null ? '—' : Math.round(pingMs).toString()}
					unit="ms"
					samples={pingHistory}
					max={300}
					reference={100}
					tone={pingMs === null ? 'text-white/40' : highIsBadTone(pingMs, 100, 250)}
				/>
				<TelemetrySparkline
					label={`${highIsBadMarker(Math.abs(clockDriftMs), 100, 500)} LAG`}
					value={Math.abs(clockDriftMs).toFixed(0)}
					unit="ms |Δ|"
					samples={clockDriftHistory}
					max={600}
					reference={100}
					tone={highIsBadTone(Math.abs(clockDriftMs), 100, 500)}
				/>
			</div>
			<dl
				class="grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1 border-t border-white/15 pt-1 tabular-nums"
			>
				<dt class="text-white/40">frame</dt>
				<dd class={highIsBadTone(p95FrameMs, 20, 40)}>
					{averageFrameMs.toFixed(1)} avg · {p95FrameMs.toFixed(1)} p95 · {maxFrameMs.toFixed(1)} max
					ms
				</dd>
				<dt class="text-white/40">slow frame</dt>
				<dd class={highIsBadTone(slowFramePercent, 5, 20)}>{slowFramePercent.toFixed(1)}% ≥20ms</dd>
				<dt class="text-white/40">net</dt>
				<dd class={lowIsBadTone(snapshotRate, 18, 10)}>
					{snapshotRate.toFixed(1)} Hz · {(snapshotBytes / 1024).toFixed(1)} KiB · {socketBufferedBytes}
					Bq
				</dd>
				<dt class="text-white/40">sim lag</dt>
				<dd class={highIsBadTone(Math.abs(clockDriftMs), 100, 500)}>
					{highIsBadMarker(Math.abs(clockDriftMs), 100, 500)}
					{Math.abs(clockDriftMs) < 100
						? 'IN SYNC'
						: Math.abs(clockDriftMs) < 500
							? 'DRIFT'
							: 'CRIT'}
					{clockDriftMs >= 0 ? '+' : ''}{clockDriftMs.toFixed(0)} ms
				</dd>
				<dt class="text-white/40">physics</dt>
				<dd>{serverPhysicsTickMs.toFixed(2)} ms/tick</dd>
				<dt class="text-white/40">stages</dt>
				<dd>
					{integrateMs.toFixed(2)} int · {broadPhaseMs.toFixed(2)} broad · {solveMs.toFixed(2)} solve
				</dd>
				<dt class="text-white/40">solver</dt>
				<dd>{candidatePairs} pair · {activeBalls} active · {sleepingBalls} sleep</dd>
				<dt class="text-white/40">server</dt>
				<dd>{serverCpuPercent.toFixed(1)}% CPU · {serverRssMiB.toFixed(0)} MiB RSS</dd>
				<dt class="text-white/40">main</dt>
				<dd
					class={mainThreadBlockedPercent === null
						? ''
						: highIsBadTone(mainThreadBlockedPercent, 5, 20)}
				>
					{mainThreadBlockedPercent === null
						? 'block n/a'
						: `${mainThreadBlockedPercent.toFixed(1)}% blocked`}
				</dd>
				<dt class="text-white/40">heap</dt>
				<dd class={heapUsagePercent === null ? '' : highIsBadTone(heapUsagePercent, 65, 85)}>
					{heapUsedMb === null || heapLimitMb === null
						? 'n/a'
						: `${heapUsedMb.toFixed(0)}/${heapLimitMb.toFixed(0)} MiB`}
				</dd>
				<dt class="text-white/40">render</dt>
				<dd>{drawCalls} calls · {renderedTriangles.toLocaleString()} tri</dd>
				<dt class="text-white/40">gpu mem</dt>
				<dd>{gpuGeometries} geo · {gpuTextures} tex</dd>
				<dt class="text-white/40">world</dt>
				<dd>
					{players.length} player · {objectFrame.positions.length / 3} obj · {contacts} contact
				</dd>
				<dt class="text-white/40">field</dt>
				<dd>
					{fieldDefinition?.colliders.length ?? 0} collision · {fieldDefinition?.triggers.length ??
						0} trigger
				</dd>
				<dt class="text-white/40">sim</dt>
				<dd>
					{simulationClock.toFixed(2)}/{matchClock.toFixed(2)} s · tick {serverTick.toLocaleString()}
				</dd>
				<dt class="text-white/40">input</dt>
				<dd>
					{controlSource} · d {inputDrive.toFixed(2)} · t {inputTurn.toFixed(2)} · i {inputIntake.toFixed(
						2
					)} · o {inputOuttake.toFixed(2)}
				</dd>
				<dt class="text-white/40">pose source</dt>
				<dd class="text-emerald-300">authoritative snapshot</dd>
				<dt class="text-white/40">camera</dt>
				<dd>{cameraMode}{cameraMode === 'robot' ? `/${cameraDirection}` : ''}</dd>
				<dt class="text-white/40">device</dt>
				<dd>
					{logicalCpuCores || '?'}T · {deviceMemoryGb === null ? 'RAM ?' : `~${deviceMemoryGb} GiB`}
				</dd>
				<dt class="text-white/40">viewport</dt>
				<dd>{viewportSpec} · {pageVisibility}</dd>
				<dt class="text-white/40">pack</dt>
				<dd class="truncate" title={packVersion}>{packVersion}</dd>
				<dt class="text-white/40">model</dt>
				<dd>
					{(physics.ballMassKg * 1000).toFixed(0)}g/{(physics.ballDiameterM * 1000).toFixed(0)}mm · {physics.floorMaterial}
				</dd>
			</dl>
			{#if players.length > 0}
				<div class="mt-1 border-t border-white/15 pt-1 text-white/45">
					{#each players as player}
						<p class="truncate">
							<span class="text-white">{player.name}</span> h{(player.headingDeg ?? 0).toFixed(1)} p({player.x.toFixed(
								2
							)},{player.y.toFixed(2)},{player.z.toFixed(2)}) v({(player.velocityX ?? 0).toFixed(
								2
							)},{(player.velocityY ?? 0).toFixed(2)},{(player.velocityZ ?? 0).toFixed(2)})
						</p>
					{/each}
				</div>
			{/if}
			{#if semanticEvents.length > 0}
				<div class="mt-1 border-t border-white/15 pt-1 text-emerald-200">
					{#each semanticEvents.slice(-4) as event}<p class="truncate">{event}</p>{/each}
				</div>
			{/if}
		</aside>
	{/if}
	<!-- Cap pixel density for the heavy imported field; this is the largest
	     client-side GPU cost on high-DPI displays. -->
	<Canvas {createRenderer} dpr={[0.65, 1]} renderMode="on-demand" shadows>
		{#if cameraMode === 'robot'}
			<RobotFollowCamera
				player={trackedPlayer}
				direction={cameraDirection}
				distance={robotCameraDistance}
			/>
		{:else}
			<T.PerspectiveCamera makeDefault position={[11, 12, 14]} fov={50}>
				<OrbitControls target={[0, 0, 0]} enablePan={false} minDistance={1} maxDistance={28} />
			</T.PerspectiveCamera>
		{/if}
		<T.AmbientLight intensity={0.55} />
		<T.DirectionalLight
			position={[8, 12, 6]}
			intensity={2.1}
			castShadow
			shadow.mapSize={[512, 512]}
			shadow.bias={-0.0005}
		/>
		<Grid
			position={[0, 0.002, 0]}
			cellColor="#64748b"
			sectionColor="#94a3b8"
			cellSize={0.5}
			sectionSize={2}
			fadeDistance={18}
		/>
		{#if fieldAssets}<PackField assets={fieldAssets} />{/if}
		{#if fieldDebugOpen && fieldDefinition}
			<FieldBoundsDebug
				colliders={fieldDefinition.colliders}
				triggers={fieldDefinition.triggers}
				boundary={fieldDefinition.boundary}
				physicsUrl={fieldAssets?.physics}
				showColliderAabbs
				{activeTriggerIds}
			/>
		{/if}
		<ScriptedObjects frame={renderedObjectFrame} />
		<T.Group>
			{#each renderedPlayers as player (player.id)}
				{#if robotAssets}
					<RobotModel
						{player}
						{physics}
						{robotAssets}
						local={player.id === localId}
						showBounds={robotBoundsDebugOpen}
					/>
				{/if}
			{/each}
		</T.Group>
	</Canvas>
</div>
