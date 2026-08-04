export type MatchPlayer = {
	id: string;
	name: string;
	teamName: string;
	color: string;
	x: number;
	y: number;
	z: number;
	yaw: number;
	headingDeg: number;
	velocityX: number;
	velocityY: number;
	velocityZ: number;
	angularVelocityY: number;
};

export type MatchPhysics = {
	ballMaterial: string;
	ballDiameterM: number;
	ballDiameterToleranceM: number;
	ballMassKg: number;
	ballFriction: number;
	ballRestitution: number;
	ballRollingResistanceMps2: number;
	floorMaterial: string;
	floorFriction: number;
	robotMassKg: number;
	robotWidthM: number;
	robotHeightM: number;
	robotLengthM: number;
	robotMaxSpeedMps: number;
	ballInertiaFactor: number;
	ballDragCoefficient: number;
	airDensityKgM3: number;
	ballBallFriction: number;
	floorStaticFriction: number;
	floorDynamicFriction: number;
	floorRollingResistanceMps2: number;
	intakeEnabled: boolean;
	intakeWidthM: number;
	intakeRadiusM: number;
	intakeForwardOffsetM: number;
	intakeCenterHeightM: number;
	intakeSurfaceSpeedMps: number;
	rampEnabled: boolean;
	rampCenterX: number;
	rampStartZ: number;
	rampWidthM: number;
	rampLengthM: number;
	rampAngleDeg: number;
	solverPositionIterations: number;
	solverVelocityIterations: number;
	maxDepenetrationSpeedMps: number;
	maxBallSpeedMps: number;
	maxBallAngularSpeedRadps: number;
	maxDriveForceN: number;
	maxDrivePowerW: number;
	maxBrakeForceN: number;
};

export type MatchSnapshot = {
	protocol: 'FGS1';
	gamePackId: string;
	gamePackVersion: string;
	objectId: string;
	objectColor: string;
	objectRadius: number;
	tick: number;
	matchClock: number;
	simulationClock: number;
	clockDriftMs: number;
	physicsTickMs: number;
	physicsLoadPercent: number;
	ticksPerSecond: number;
	targetTicksPerSecond: number;
	contacts: number;
	integrateMs: number;
	broadPhaseMs: number;
	solveMs: number;
	candidatePairs: number;
	activeBalls: number;
	sleepingBalls: number;
	serverCpuPercent: number;
	serverRssMiB: number;
	players: MatchPlayer[];
	positions: Float32Array;
	physics?: MatchPhysics;
};

const decoder = new TextDecoder();

class Reader {
	readonly view: DataView;
	offset: number;

	constructor(view: DataView, offset = 0) {
		this.view = view;
		this.offset = offset;
	}

	u16() {
		const value = this.view.getUint16(this.offset, true);
		this.offset += 2;
		return value;
	}

	u8() {
		const value = this.view.getUint8(this.offset);
		this.offset += 1;
		return value;
	}

	u32() {
		const value = this.view.getUint32(this.offset, true);
		this.offset += 4;
		return value;
	}

	u64() {
		const value = Number(this.view.getBigUint64(this.offset, true));
		this.offset += 8;
		return value;
	}

	f32() {
		const value = this.view.getFloat32(this.offset, true);
		this.offset += 4;
		return value;
	}

	f64() {
		const value = this.view.getFloat64(this.offset, true);
		this.offset += 8;
		return value;
	}

	string() {
		const length = this.u16();
		const bytes = new Uint8Array(this.view.buffer, this.view.byteOffset + this.offset, length);
		this.offset += length;
		return decoder.decode(bytes);
	}
}

/**
 * Decode the FGS1 sectioned little-endian protocol. Unknown section tags are
 * skipped using their byte length, so compatible protocol additions do not
 * require coordinated server/client deployment.
 */
export function decodeMatchSnapshot(buffer: ArrayBuffer): MatchSnapshot {
	if (buffer.byteLength < 16) throw new Error('Match snapshot is shorter than its header');
	const magic = decoder.decode(new Uint8Array(buffer, 0, 4));
	if (magic !== 'FGS1') throw new Error(`Unsupported match protocol ${magic}`);
	const header = new Reader(new DataView(buffer), 4);
	const major = header.u16();
	header.u16(); // compatible minor version
	const messageType = header.u16();
	header.u16(); // flags
	const payloadLength = header.u32();
	if (major !== 1 || messageType !== 1) throw new Error('Unsupported match snapshot version');
	if (payloadLength > buffer.byteLength - 16) throw new Error('Truncated match snapshot');

	const snapshot: MatchSnapshot = {
		protocol: 'FGS1',
		gamePackId: 'unknown',
		gamePackVersion: 'unknown',
		objectId: 'object',
		objectColor: '#f97316',
		objectRadius: 0.05,
		tick: 0,
		matchClock: 0,
		simulationClock: 0,
		clockDriftMs: 0,
		physicsTickMs: 0,
		physicsLoadPercent: 0,
		ticksPerSecond: 0,
		targetTicksPerSecond: 60,
		contacts: 0,
		integrateMs: 0,
		broadPhaseMs: 0,
		solveMs: 0,
		candidatePairs: 0,
		activeBalls: 0,
		sleepingBalls: 0,
		serverCpuPercent: 0,
		serverRssMiB: 0,
		players: [],
		positions: new Float32Array()
	};

	const view = new DataView(buffer);
	let offset = 16;
	const end = 16 + payloadLength;
	while (offset + 8 <= end) {
		const sectionHeader = new Reader(view, offset);
		const tag = sectionHeader.u16();
		sectionHeader.u16();
		const length = sectionHeader.u32();
		const sectionStart = offset + 8;
		const sectionEnd = sectionStart + length;
		if (sectionEnd > end) throw new Error('Truncated match snapshot section');
		const section = new Reader(view, sectionStart);

		switch (tag) {
			case 1:
				snapshot.gamePackId = section.string();
				snapshot.gamePackVersion = section.string();
				snapshot.objectId = section.string();
				snapshot.objectColor = section.string();
				snapshot.objectRadius = section.f32();
				break;
			case 2:
				snapshot.tick = section.u64();
				snapshot.matchClock = section.f64();
				snapshot.simulationClock = section.f64();
				snapshot.clockDriftMs = section.f64();
				break;
			case 3:
				snapshot.physicsTickMs = section.f64();
				snapshot.physicsLoadPercent = section.f64();
				snapshot.ticksPerSecond = section.f64();
				snapshot.targetTicksPerSecond = section.f64();
				snapshot.contacts = section.u32();
				snapshot.integrateMs = section.f64();
				snapshot.broadPhaseMs = section.f64();
				snapshot.solveMs = section.f64();
				snapshot.candidatePairs = section.u32();
				snapshot.activeBalls = section.u32();
				snapshot.sleepingBalls = section.u32();
				if (section.offset + 16 <= sectionEnd) {
					snapshot.serverCpuPercent = section.f64();
					snapshot.serverRssMiB = section.f64();
				}
				break;
			case 4: {
				const count = section.u32();
				const players: MatchPlayer[] = [];
				for (let index = 0; index < count; index += 1) {
					const id = section.string();
					const name = section.string();
					const teamName = section.string();
					const color = section.string();
					players.push({
						id,
						name,
						teamName,
						color,
						x: section.f32(),
						y: section.f32(),
						z: section.f32(),
						yaw: section.f32(),
						headingDeg: section.f32(),
						velocityX: section.f32(),
						velocityY: section.f32(),
						velocityZ: section.f32(),
						angularVelocityY: section.f32()
					});
				}
				snapshot.players = players;
				break;
			}
			case 5: {
				const count = section.u32();
				const positions = new Float32Array(count * 3);
				for (let index = 0; index < positions.length; index += 1) positions[index] = section.f32();
				snapshot.positions = positions;
				break;
			}
			case 6:
				const physics: MatchPhysics = {
					ballMaterial: section.string(),
					floorMaterial: section.string(),
					ballDiameterM: section.f32(),
					ballDiameterToleranceM: section.f32(),
					ballMassKg: section.f32(),
					ballFriction: section.f32(),
					ballRestitution: section.f32(),
					ballRollingResistanceMps2: section.f32(),
					floorFriction: section.f32(),
					robotMassKg: section.f32(),
					robotWidthM: section.f32(),
					robotHeightM: section.f32(),
					robotLengthM: section.f32(),
					robotMaxSpeedMps: section.f32(),
					ballInertiaFactor: 0.4,
					ballDragCoefficient: 0.47,
					airDensityKgM3: 1.225,
					ballBallFriction: 0.35,
					floorStaticFriction: 0.85,
					floorDynamicFriction: 0.65,
					floorRollingResistanceMps2: 0.55,
					intakeEnabled: false,
					intakeWidthM: 0.52,
					intakeRadiusM: 0.045,
					intakeForwardOffsetM: 0.38,
					intakeCenterHeightM: 0.075,
					intakeSurfaceSpeedMps: 3,
					rampEnabled: false,
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
					maxBrakeForceN: 200
				};
				if (section.offset + 70 <= sectionEnd) {
					physics.intakeEnabled = section.u8() !== 0;
					physics.rampEnabled = section.u8() !== 0;
					physics.ballInertiaFactor = section.f32();
					physics.ballDragCoefficient = section.f32();
					physics.airDensityKgM3 = section.f32();
					physics.ballBallFriction = section.f32();
					physics.floorStaticFriction = section.f32();
					physics.floorDynamicFriction = section.f32();
					physics.floorRollingResistanceMps2 = section.f32();
					physics.intakeWidthM = section.f32();
					physics.intakeRadiusM = section.f32();
					physics.intakeForwardOffsetM = section.f32();
					physics.intakeCenterHeightM = section.f32();
					physics.intakeSurfaceSpeedMps = section.f32();
					physics.rampCenterX = section.f32();
					physics.rampStartZ = section.f32();
					physics.rampWidthM = section.f32();
					physics.rampLengthM = section.f32();
					physics.rampAngleDeg = section.f32();
				}
				if (section.offset + 32 <= sectionEnd) {
					physics.solverPositionIterations = section.f32();
					physics.solverVelocityIterations = section.f32();
					physics.maxDepenetrationSpeedMps = section.f32();
					physics.maxBallSpeedMps = section.f32();
					physics.maxBallAngularSpeedRadps = section.f32();
					physics.maxDriveForceN = section.f32();
					physics.maxDrivePowerW = section.f32();
					physics.maxBrakeForceN = section.f32();
				}
				snapshot.physics = physics;
				break;
		}
		offset = sectionEnd;
	}
	return snapshot;
}
