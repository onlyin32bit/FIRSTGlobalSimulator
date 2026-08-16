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
	id: string;
	min: [number, number, number];
	max: [number, number, number];
	center: [number, number, number];
	halfExtents: [number, number, number];
	axes: [[number, number, number], [number, number, number], [number, number, number]];
};

export type DriveParams = {
	maxSpeedMps: number;
	maxAccelerationMps2: number;
	maxDecelerationMps2: number;
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
	robotColliders: FieldCollider[];
};

type V3 = [number, number, number];

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

const dot3 = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

const sub3 = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

const mul3 = (a: V3, s: number): V3 => [a[0] * s, a[1] * s, a[2] * s];

const cross3 = (a: V3, b: V3): V3 => [
	a[1] * b[2] - a[2] * b[1],
	a[2] * b[0] - a[0] * b[2],
	a[0] * b[1] - a[1] * b[0]
];

const GRAVITY = 9.81;
const CONTROL_DEADBAND = 0.08;
const TURN_BRAKE_MULTIPLIER = 2.5;
const TURN_STOP_EPSILON_RADPS = 0.04;

const applyControlDeadband = (value: number) => {
	const clamped = clamp(value, -1, 1);
	const magnitude = Math.abs(clamped);
	if (magnitude <= CONTROL_DEADBAND) return 0;
	return Math.sign(clamped) * ((magnitude - CONTROL_DEADBAND) / (1 - CONTROL_DEADBAND));
};

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

const authoredRobotCollider = (
	local: FieldCollider,
	pose: RobotPose,
	heightM: number
): FieldCollider => {
	const sin = Math.sin(pose.yaw);
	const cos = Math.cos(pose.yaw);
	const rotate = (value: V3): V3 => {
		const corrected: V3 = [-value[0], value[1], -value[2]];
		return [
			cos * corrected[0] + sin * corrected[2],
			corrected[1],
			-sin * corrected[0] + cos * corrected[2]
		];
	};
	const offset = rotate([local.center[0], local.center[1] - heightM * 0.5, local.center[2]]);
	const center: V3 = [pose.x + offset[0], pose.y + offset[1], pose.z + offset[2]];
	const axes = local.axes.map((axis) => rotate(axis)) as FieldCollider['axes'];
	const min = [...center] as V3;
	const max = [...center] as V3;
	for (let worldAxis = 0; worldAxis < 3; worldAxis += 1) {
		const radius = axes.reduce(
			(sum, axis, localAxis) => sum + Math.abs(axis[worldAxis]) * local.halfExtents[localAxis],
			0
		);
		min[worldAxis] -= radius;
		max[worldAxis] += radius;
	}
	return { ...local, center, axes, min, max };
};

const obbContact = (
	left: FieldCollider,
	right: FieldCollider
): { normal: V3; penetration: number } | null => {
	const axes: V3[] = [...left.axes, ...right.axes];
	for (const leftAxis of left.axes) {
		for (const rightAxis of right.axes) {
			const candidate = cross3(leftAxis, rightAxis);
			const candidateLength = Math.hypot(...candidate);
			if (candidateLength > 1e-5) axes.push(mul3(candidate, 1 / candidateLength));
		}
	}
	const centerDelta = sub3(left.center, right.center);
	let minimumPenetration = Infinity;
	let minimumNormal: V3 = [0, 1, 0];
	for (const axis of axes) {
		const leftRadius = left.axes.reduce(
			(sum, localAxis, index) => sum + left.halfExtents[index] * Math.abs(dot3(axis, localAxis)),
			0
		);
		const rightRadius = right.axes.reduce(
			(sum, localAxis, index) => sum + right.halfExtents[index] * Math.abs(dot3(axis, localAxis)),
			0
		);
		const penetration = leftRadius + rightRadius - Math.abs(dot3(centerDelta, axis));
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
	let authoredColliders = params.robotColliders.map((local) =>
		authoredRobotCollider(local, p, params.heightM)
	);

	for (const collider of params.colliders) {
		if (authoredColliders.length) {
			let contact: { normal: V3; penetration: number } | null = null;
			for (const robot of authoredColliders) {
				const overlaps =
					robot.min.every((value, axis) => value <= collider.max[axis]) &&
					robot.max.every((value, axis) => value >= collider.min[axis]);
				if (!overlaps) continue;
				const candidate = obbContact(robot, collider);
				if (candidate && (!contact || candidate.penetration > contact.penetration)) {
					contact = candidate;
				}
			}
			if (!contact) continue;
			p.x += contact.normal[0] * contact.penetration;
			p.y += contact.normal[1] * contact.penetration;
			p.z += contact.normal[2] * contact.penetration;
			const intoSurface = p.vx * contact.normal[0] + p.vz * contact.normal[2];
			if (intoSurface < 0) {
				p.vx -= contact.normal[0] * intoSurface;
				p.vz -= contact.normal[2] * intoSurface;
			}
			authoredColliders = params.robotColliders.map((local) =>
				authoredRobotCollider(local, p, params.heightM)
			);
			continue;
		}
		if (robotMaxY <= collider.min[1] || robotMinY >= collider.max[1]) continue;

		if (collider.halfExtents.some((extent) => extent > 1.0e-6)) {
			const contact = robotFieldObbContact([p.x, p.y, p.z], p.yaw, [halfX, halfY, halfZ], collider);
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

const projectBoundary = (p: RobotPose, params: DriveParams) => {
	let minX: number;
	let maxX: number;
	let minZ: number;
	let maxZ: number;
	if (params.robotColliders.length) {
		const colliders = params.robotColliders.map((local) =>
			authoredRobotCollider(local, p, params.heightM)
		);
		minX = Math.min(...colliders.map((collider) => collider.min[0]));
		maxX = Math.max(...colliders.map((collider) => collider.max[0]));
		minZ = Math.min(...colliders.map((collider) => collider.min[2]));
		maxZ = Math.max(...colliders.map((collider) => collider.max[2]));
	} else {
		const [extentX, extentZ] = robotPlanarExtents(params.widthM, params.lengthM, p.yaw);
		minX = p.x - extentX;
		maxX = p.x + extentX;
		minZ = p.z - extentZ;
		maxZ = p.z + extentZ;
	}

	if (minX < params.boundaryMinX) {
		p.x += params.boundaryMinX - minX;
		if (p.vx < 0) p.vx = 0;
	} else if (maxX > params.boundaryMaxX) {
		p.x -= maxX - params.boundaryMaxX;
		if (p.vx > 0) p.vx = 0;
	}
	if (minZ < params.boundaryMinZ) {
		p.z += params.boundaryMinZ - minZ;
		if (p.vz < 0) p.vz = 0;
	} else if (maxZ > params.boundaryMaxZ) {
		p.z -= maxZ - params.boundaryMaxZ;
		if (p.vz > 0) p.vz = 0;
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
	private authoritative: RobotPose;
	pose: RobotPose;

	constructor(params: DriveParams, initial: RobotPose) {
		this.params = params;
		this.pose = { ...initial };
		this.authoritative = { ...initial };
	}

	setPose(pose: RobotPose) {
		this.pose = { ...pose };
		this.authoritative = { ...pose };
	}

	reconcile(pose: RobotPose) {
		const distance = Math.hypot(pose.x - this.pose.x, pose.z - this.pose.z);
		this.authoritative = { ...pose };
		if (distance > 3 || Math.abs(pose.y - this.pose.y) > 1) this.setPose(pose);
		return distance;
	}

	step(input: DriveInput, dt: number) {
		const frameDt = clamp(dt, 0, 0.05);
		this.authoritative.x += this.authoritative.vx * frameDt;
		this.authoritative.z += this.authoritative.vz * frameDt;
		this.authoritative.yaw += this.authoritative.angularVelocityY * frameDt;
		let remaining = frameDt;
		while (remaining > 1e-6) {
			const substep = Math.min(remaining, 1 / 60);
			this.stepPlanar(input, substep);
			remaining -= substep;
		}

		// The local model cannot reproduce Rapier's carpet contacts exactly.
		// A damped target follows the latest server velocity and removes drift
		// continuously instead of periodically snapping the rendered chassis.
		const positionPull = 1 - Math.exp(-8 * frameDt);
		const velocityPull = 1 - Math.exp(-12 * frameDt);
		const yawDelta = Math.atan2(
			Math.sin(this.authoritative.yaw - this.pose.yaw),
			Math.cos(this.authoritative.yaw - this.pose.yaw)
		);
		this.pose.x += (this.authoritative.x - this.pose.x) * positionPull;
		this.pose.z += (this.authoritative.z - this.pose.z) * positionPull;
		this.pose.y = this.authoritative.y;
		this.pose.yaw += yawDelta * positionPull;
		this.pose.vx += (this.authoritative.vx - this.pose.vx) * velocityPull;
		this.pose.vz += (this.authoritative.vz - this.pose.vz) * velocityPull;
		this.pose.angularVelocityY +=
			(this.authoritative.angularVelocityY - this.pose.angularVelocityY) * velocityPull;
	}

	private stepPlanar(input: DriveInput, stepDt: number) {
		const {
			maxSpeedMps,
			maxAccelerationMps2,
			maxDecelerationMps2,
			maxTurnRateRadps,
			maxAngularAccelerationRadps2,
			lateralGripMps2,
			tractionFriction,
			trackWidthM
		} = this.params;
		const p = this.pose;
		const drive = applyControlDeadband(input.drive);
		const turn = applyControlDeadband(input.turn);

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
		let leftPower = drive + turn;
		let rightPower = drive - turn;
		const peakPower = Math.max(Math.abs(leftPower), Math.abs(rightPower), 1);
		leftPower /= peakPower;
		rightPower /= peakPower;

		const targetSpeed = (leftPower + rightPower) * 0.5 * maxSpeedMps;
		const braking =
			Math.abs(targetSpeed) < Math.abs(forwardSpeed) ||
			Math.sign(targetSpeed) !== Math.sign(forwardSpeed);
		const accelLimit = Math.min(
			braking ? maxDecelerationMps2 : maxAccelerationMps2,
			tractionFriction * GRAVITY
		);
		const forwardDelta = clamp(
			targetSpeed - forwardSpeed,
			-accelLimit * stepDt,
			accelLimit * stepDt
		);
		const lateralAcceleration = Math.min(lateralGripMps2, tractionFriction * GRAVITY);
		const lateralDelta = clamp(
			-lateralSpeed,
			-lateralAcceleration * stepDt,
			lateralAcceleration * stepDt
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
		const turnAcceleration =
			Math.abs(turn) <= Number.EPSILON
				? maxAngularAccelerationRadps2 * TURN_BRAKE_MULTIPLIER
				: maxAngularAccelerationRadps2;
		const turnDelta = clamp(
			targetTurnRate - p.angularVelocityY,
			-turnAcceleration * stepDt,
			turnAcceleration * stepDt
		);
		p.angularVelocityY += turnDelta;
		if (
			Math.abs(turn) <= Number.EPSILON &&
			Math.abs(p.angularVelocityY) < TURN_STOP_EPSILON_RADPS
		) {
			p.angularVelocityY = 0;
		}

		p.x += p.vx * stepDt;
		p.z += p.vz * stepDt;
		p.yaw += p.angularVelocityY * stepDt;

		// The server never lets the chassis cross the perimeter: clamp to the
		// rotated-footprint clearance and cancel the velocity into the wall so
		// the predicted pose stays on the playable carpet and slides along it.
		projectBoundary(p, this.params);

		projectFieldColliders(p, this.params);
	}
}
