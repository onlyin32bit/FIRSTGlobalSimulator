export type DriveInput = {
	turn: number;
	drive: number;
};

export type RobotPose = {
	x: number;
	y: number;
	z: number;
	yaw: number;
	vx: number;
	vz: number;
	angularVelocityY: number;
};

export type FieldCollider = {
	min: [number, number, number];
	max: [number, number, number];
	center: [number, number, number];
	halfExtents: [number, number, number];
	axes: [[number, number, number], [number, number, number], [number, number, number]];
};

export type DriveParams = {
	massKg: number;
	maxSpeedMps: number;
	maxAccelerationMps2: number;
	maxDecelerationMps2: number;
	maxDriveForceN: number;
	maxDrivePowerW: number;
	maxBrakeForceN: number;
	rollingResistanceMps2: number;
	maxTurnRateRadps: number;
	maxAngularAccelerationRadps2: number;
	lateralGripMps2: number;
	tractionFriction: number;
	trackWidthM: number;
	widthM: number;
	lengthM: number;
	heightM: number;
	boundaryMinX: number;
	boundaryMaxX: number;
	boundaryMinZ: number;
	boundaryMaxZ: number;
	colliders: FieldCollider[];
};

type V3 = [number, number, number];

const clamp = (value: number, min: number, max: number) =>
	Math.min(max, Math.max(min, value));

const dot3 = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

const sub3 = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

const mul3 = (a: V3, s: number): V3 => [a[0] * s, a[1] * s, a[2] * s];

const cross3 = (a: V3, b: V3): V3 => [
	a[1] * b[2] - a[2] * b[1],
	a[2] * b[0] - a[0] * b[2],
	a[0] * b[1] - a[1] * b[0]
];

const GRAVITY = 9.81;

/**
 * Projected planar half-extents of the rotated chassis, mirroring the
 * server's `robot_planar_extents`. The perimeter clearance shrinks along one
 * axis and grows along the other as the robot turns, so a plain AABB would
 * let the corner swing through the wall.
 */
const robotPlanarExtents = (widthM: number, lengthM: number, yaw: number): [number, number] => {
	const halfX = widthM * 0.5;
	const halfZ = lengthM * 0.5;
	const cos = Math.abs(Math.cos(yaw));
	const sin = Math.abs(Math.sin(yaw));
	return [halfX * cos + halfZ * sin, halfX * sin + halfZ * cos];
};

/**
 * Minimum-translation SAT contact between the rotated robot box and one
 * authored field OBB. Mirrors the server's `robot_field_obb_contact` exactly
 * so the predictor resolves interior obstacles (ramp, riser) the same way the
 * authoritative solver does.
 */
const robotFieldObbContact = (
	robotCenter: V3,
	robotYaw: number,
	robotHalf: V3,
	collider: FieldCollider
): { normal: V3; penetration: number } | null => {
	const sin = Math.sin(robotYaw);
	const cos = Math.cos(robotYaw);
	const robotAxes: V3[] = [
		[cos, 0, -sin],
		[0, 1, 0],
		[sin, 0, cos]
	];
	const colliderAxes = collider.axes;
	const axes: V3[] = [...robotAxes, ...colliderAxes];
	for (const robotAxis of robotAxes) {
		for (const colliderAxis of colliderAxes) {
			const candidate = cross3(robotAxis, colliderAxis);
			const length = Math.hypot(candidate[0], candidate[1], candidate[2]);
			if (length <= 1.0e-5) continue;
			axes.push(mul3(candidate, 1 / length));
		}
	}

	const centerDelta = sub3(robotCenter, collider.center);
	let minimumPenetration = Infinity;
	let minimumNormal: V3 = [0, 1, 0];
	for (const axis of axes) {
		const robotRadius =
			robotHalf[0] * Math.abs(dot3(axis, robotAxes[0])) +
			robotHalf[1] * Math.abs(dot3(axis, robotAxes[1])) +
			robotHalf[2] * Math.abs(dot3(axis, robotAxes[2]));
		const colliderRadius =
			collider.halfExtents[0] * Math.abs(dot3(axis, colliderAxes[0])) +
			collider.halfExtents[1] * Math.abs(dot3(axis, colliderAxes[1])) +
			collider.halfExtents[2] * Math.abs(dot3(axis, colliderAxes[2]));
		const penetration = robotRadius + colliderRadius - Math.abs(dot3(centerDelta, axis));
		if (penetration <= 0) return null;
		if (penetration < minimumPenetration) {
			minimumPenetration = penetration;
			minimumNormal = dot3(centerDelta, axis) < 0 ? mul3(axis, -1) : axis;
		}
	}
	return { normal: minimumNormal, penetration: minimumPenetration };
};

/**
 * Push the robot out of any interior field collider it overlaps and zero the
 * velocity component driving into the surface. Mirrors the server's
 * `project_robot_field_colliders`: OBBs use SAT, the authored guard rails are
 * treated as AABBs pushed out along the smallest penetration.
 */
const projectFieldColliders = (p: RobotPose, params: DriveParams) => {
	const halfX = params.widthM * 0.5;
	const halfZ = params.lengthM * 0.5;
	const halfY = params.heightM * 0.5;
	const robotMinY = p.y - halfY;
	const robotMaxY = p.y + halfY;

	for (const collider of params.colliders) {
		if (robotMaxY <= collider.min[1] || robotMinY >= collider.max[1]) continue;

		if (collider.halfExtents.some((extent) => extent > 1.0e-6)) {
			const contact = robotFieldObbContact(
				[p.x, p.y, p.z],
				p.yaw,
				[halfX, halfY, halfZ],
				collider
			);
			if (!contact) continue;
			p.x += contact.normal[0] * contact.penetration;
			p.y += contact.normal[1] * contact.penetration;
			p.z += contact.normal[2] * contact.penetration;
			const intoSurface = p.vx * contact.normal[0] + p.vz * contact.normal[2];
			if (intoSurface < 0) {
				p.vx -= contact.normal[0] * intoSurface;
				p.vz -= contact.normal[2] * intoSurface;
			}
			continue;
		}

		const robotMinX = p.x - halfX;
		const robotMaxX = p.x + halfX;
		const robotMinZ = p.z - halfZ;
		const robotMaxZ = p.z + halfZ;
		if (
			robotMaxX <= collider.min[0] ||
			robotMinX >= collider.max[0] ||
			robotMaxZ <= collider.min[2] ||
			robotMinZ >= collider.max[2]
		) {
			continue;
		}

		const pushLeft = robotMaxX - collider.min[0];
		const pushRight = collider.max[0] - robotMinX;
		const pushBack = robotMaxZ - collider.min[2];
		const pushFront = collider.max[2] - robotMinZ;
		const candidates: Array<[number, V3]> = [
			[pushLeft, [-1, 0, 0]],
			[pushRight, [1, 0, 0]],
			[pushBack, [0, 0, -1]],
			[pushFront, [0, 0, 1]]
		];
		const [distance, normal] = candidates.reduce((least, candidate) =>
			candidate[0] < least[0] ? candidate : least
		);
		p.x += normal[0] * Math.max(distance, 0);
		p.z += normal[2] * Math.max(distance, 0);
		const intoSurface = p.vx * normal[0] + p.vz * normal[2];
		if (intoSurface < 0) {
			p.vx -= normal[0] * intoSurface;
			p.vz -= normal[2] * intoSurface;
		}
	}
};

/**
 * Local reproduction of the server's `apply_player_drive` drivetrain model.
 * The server integrates the same impulse/turn logic on its authoritative
 * physics step, then clamps the robot to the playable perimeter and projects
 * it out of interior field colliders. The predictor mirrors all three so the
 * rendered robot never drifts outside the arena — eliminating the through-wall
 * pass-through that previously ended in a hard snap-back. The server snapshot
 * reconciles any remaining divergence (e.g. ball contacts).
 */
export class DrivePredictor {
	private readonly params: DriveParams;
	private accumulatedTime = 0;
	pose: RobotPose;

	constructor(params: DriveParams, initial: RobotPose) {
		this.params = params;
		this.pose = { ...initial };
	}

	setPose(pose: RobotPose) {
		this.pose = { ...pose };
	}

	step(input: DriveInput, dt: number) {
		// The authoritative server advances the robot at a fixed 60 Hz. Keep
		// prediction on the same lattice so variable browser frame rates do not
		// create a second, slightly different physics clock.
		const fixedDt = 1 / 60;
		this.accumulatedTime = Math.min(this.accumulatedTime + clamp(dt, 0, 0.05), 0.25);
		while (this.accumulatedTime >= fixedDt) {
			this.stepFixed(input, fixedDt);
			this.accumulatedTime -= fixedDt;
		}
	}

	private stepFixed(input: DriveInput, stepDt: number) {
		const {
			massKg,
			maxSpeedMps,
			maxAccelerationMps2,
			maxDecelerationMps2,
			maxDriveForceN,
			maxDrivePowerW,
			maxBrakeForceN,
			rollingResistanceMps2,
			maxTurnRateRadps,
			maxAngularAccelerationRadps2,
			lateralGripMps2,
			tractionFriction,
			trackWidthM
		} = this.params;
		const p = this.pose;
		// Robot is constrained to yaw only; forward/right follow the same
		// quaternion expansion the server derives from Rapier's rotation.
		const forwardX = -Math.sin(p.yaw);
		const forwardZ = -Math.cos(p.yaw);
		const rightX = Math.cos(p.yaw);
		const rightZ = -Math.sin(p.yaw);

		const forwardSpeed = p.vx * forwardX + p.vz * forwardZ;
		const lateralSpeed = p.vx * rightX + p.vz * rightZ;

		// Arcade input → differential wheel power, peak-normalised so hard
		// steering scrubs forward drive exactly like the real drivetrain.
		let leftPower = input.drive + input.turn;
		let rightPower = input.drive - input.turn;
		const peakPower = Math.max(Math.abs(leftPower), Math.abs(rightPower), 1);
		leftPower /= peakPower;
		rightPower /= peakPower;

		const targetSpeed = (leftPower + rightPower) * 0.5 * maxSpeedMps;
		const braking =
			Math.abs(targetSpeed) < Math.abs(forwardSpeed) ||
			Math.sign(targetSpeed) !== Math.sign(forwardSpeed) ||
			Math.abs(targetSpeed) < 1.0e-4;
		const tractionLimit = Math.max(tractionFriction, 0) * massKg * GRAVITY;
		const forceLimit = Math.min(
			braking
				? maxBrakeForceN
					: Math.min(
							maxDriveForceN,
							massKg * maxAccelerationMps2,
							maxDrivePowerW /
								Math.max(Math.abs(forwardSpeed), maxSpeedMps * 0.08, 0.1)
						),
				braking ? massKg * maxDecelerationMps2 : tractionLimit,
			tractionLimit
		);
		const requestedForce =
			((targetSpeed - forwardSpeed) * massKg) / Math.max(stepDt, 1.0e-5);
		const forwardDelta =
			(clamp(requestedForce, -forceLimit, forceLimit) / Math.max(massKg, 1.0)) * stepDt;
		const lateralDelta = clamp(
			-lateralSpeed,
			-lateralGripMps2 * stepDt,
			lateralGripMps2 * stepDt
		);

		// Impulse over mass equals the velocity delta; the server applies
		// impulse = Δv · mass, so these are directly comparable.
		p.vx += forwardX * forwardDelta + rightX * lateralDelta;
		p.vz += forwardZ * forwardDelta + rightZ * lateralDelta;

		const wheelDelta = rightPower - leftPower;
		const targetTurnRate = clamp(
			(wheelDelta * maxSpeedMps) / Math.max(trackWidthM, 0.1),
			-maxTurnRateRadps,
			maxTurnRateRadps
		);
		const turnDelta = clamp(
			targetTurnRate - p.angularVelocityY,
			-maxAngularAccelerationRadps2 * stepDt,
			maxAngularAccelerationRadps2 * stepDt
		);
		p.angularVelocityY += turnDelta;

		p.x += p.vx * stepDt;
		p.z += p.vz * stepDt;
		p.yaw += p.angularVelocityY * stepDt;

		// The server never lets the chassis cross the perimeter: clamp to the
		// rotated-footprint clearance and cancel the velocity into the wall so
		// the predicted pose stays on the playable carpet and slides along it.
		const [robotXExtent, robotZExtent] = robotPlanarExtents(
			this.params.widthM,
			this.params.lengthM,
			p.yaw
		);
		const minX = this.params.boundaryMinX + robotXExtent;
		const maxX = this.params.boundaryMaxX - robotXExtent;
		const minZ = this.params.boundaryMinZ + robotZExtent;
		const maxZ = this.params.boundaryMaxZ - robotZExtent;
		if (p.x <= minX + 1.0e-6 || p.x >= maxX - 1.0e-6) {
			const normal = p.x <= minX + 1.0e-6 ? 1 : -1;
			const intoSurface = p.vx * normal;
			if (intoSurface < 0) p.vx -= normal * intoSurface;
			p.x = clamp(p.x, minX, maxX);
		}
		if (p.z <= minZ + 1.0e-6 || p.z >= maxZ - 1.0e-6) {
			const normal = p.z <= minZ + 1.0e-6 ? 1 : -1;
			const intoSurface = p.vz * normal;
			if (intoSurface < 0) p.vz -= normal * intoSurface;
			p.z = clamp(p.z, minZ, maxZ);
		}

		projectFieldColliders(p, this.params);

		const drag = Math.exp(-Math.max(rollingResistanceMps2, 0) * stepDt);
		p.vx *= drag;
		p.vz *= drag;
	}
}
