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
	robotColliders?: FieldCollider[];
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
 * server's `robot_planar_extents`.
 */
const robotPlanarExtents = (widthM: number, lengthM: number, yaw: number): [number, number] => {
	const halfX = widthM * 0.5;
	const halfZ = lengthM * 0.5;
	const cos = Math.abs(Math.cos(yaw));
	const sin = Math.abs(Math.sin(yaw));
	return [halfX * cos + halfZ * sin, halfX * sin + halfZ * cos];
};

/**
 * Minimum-translation SAT contact between two OBBs.
 * Mirrors the server's `obb_obb_contact` exactly.
 */
export const obbObbContact = (
	centerA: V3,
	axesA: [V3, V3, V3],
	halfA: V3,
	centerB: V3,
	axesB: [V3, V3, V3],
	halfB: V3
): { normal: V3; penetration: number } | null => {
	const axes: V3[] = [...axesA, ...axesB];
	for (const axisA of axesA) {
		for (const axisB of axesB) {
			const candidate = cross3(axisA, axisB);
			const length = Math.hypot(candidate[0], candidate[1], candidate[2]);
			if (length <= 1.0e-5) continue;
			axes.push(mul3(candidate, 1 / length));
		}
	}

	const centerDelta = sub3(centerA, centerB);
	let minimumPenetration = Infinity;
	let minimumNormal: V3 = [0, 1, 0];
	for (const axis of axes) {
		const radiusA =
			halfA[0] * Math.abs(dot3(axis, axesA[0])) +
			halfA[1] * Math.abs(dot3(axis, axesA[1])) +
			halfA[2] * Math.abs(dot3(axis, axesA[2]));
		const radiusB =
			halfB[0] * Math.abs(dot3(axis, axesB[0])) +
			halfB[1] * Math.abs(dot3(axis, axesB[1])) +
			halfB[2] * Math.abs(dot3(axis, axesB[2]));
		const penetration = radiusA + radiusB - Math.abs(dot3(centerDelta, axis));
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
 * velocity component driving into the surface.
 * Mirrors the server's `project_robot_field_colliders` exactly.
 */
const projectFieldColliders = (p: RobotPose, params: DriveParams) => {
	const yaw = p.yaw + Math.PI;
	const sin = Math.sin(yaw);
	const cos = Math.cos(yaw);
	const rotate = (v: V3): V3 => [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];

	const fallbackCollider: FieldCollider = {
		min: [-params.widthM * 0.5, -params.heightM * 0.5, -params.lengthM * 0.5],
		max: [params.widthM * 0.5, params.heightM * 0.5, params.lengthM * 0.5],
		center: [0, 0, 0],
		halfExtents: [params.widthM * 0.5, params.heightM * 0.5, params.lengthM * 0.5],
		axes: [
			[1, 0, 0],
			[0, 1, 0],
			[0, 0, 1]
		]
	};

	const subColliders =
		params.robotColliders && params.robotColliders.length > 0
			? params.robotColliders
			: [fallbackCollider];

	for (const subColl of subColliders) {
		const subCenter: V3 = [
			p.x + cos * subColl.center[0] + sin * subColl.center[2],
			p.y + subColl.center[1],
			p.z - sin * subColl.center[0] + cos * subColl.center[2]
		];
		const subAxes: [V3, V3, V3] = [
			rotate(subColl.axes[0]),
			rotate(subColl.axes[1]),
			rotate(subColl.axes[2])
		];
		const subHalf = subColl.halfExtents;

		const subExtentY =
			Math.abs(subAxes[0][1]) * subHalf[0] +
			Math.abs(subAxes[1][1]) * subHalf[1] +
			Math.abs(subAxes[2][1]) * subHalf[2];
		const subMinY = subCenter[1] - subExtentY;
		const subMaxY = subCenter[1] + subExtentY;

		for (const collider of params.colliders) {
			if (subMaxY <= collider.min[1] || subMinY >= collider.max[1]) continue;

			let fieldCenter: V3;
			let fieldAxes: [V3, V3, V3];
			let fieldHalf: V3;

			if (collider.halfExtents.some((extent) => extent > 1.0e-6)) {
				fieldCenter = collider.center;
				fieldAxes = collider.axes;
				fieldHalf = collider.halfExtents;
			} else {
				fieldCenter = [
					(collider.min[0] + collider.max[0]) * 0.5,
					(collider.min[1] + collider.max[1]) * 0.5,
					(collider.min[2] + collider.max[2]) * 0.5
				];
				fieldHalf = [
					(collider.max[0] - collider.min[0]) * 0.5,
					(collider.max[1] - collider.min[1]) * 0.5,
					(collider.max[2] - collider.min[2]) * 0.5
				];
				fieldAxes = [
					[1, 0, 0],
					[0, 1, 0],
					[0, 0, 1]
				];
			}

			const contact = obbObbContact(
				subCenter,
				subAxes,
				subHalf,
				fieldCenter,
				fieldAxes,
				fieldHalf
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
		}
	}
};

/**
 * Local reproduction of the server's `apply_player_drive` drivetrain model.
 */
export class DrivePredictor {
	private readonly params: DriveParams;
	pose: RobotPose;

	constructor(params: DriveParams, initial: RobotPose) {
		this.params = params;
		this.pose = { ...initial };
	}

	updateRobotColliders(robotColliders: FieldCollider[]) {
		this.params.robotColliders = robotColliders;
	}

	setPose(pose: RobotPose) {
		this.pose = { ...pose };
	}

	step(input: DriveInput, dt: number) {
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
		const stepDt = clamp(dt, 0, 0.05);

		const forwardX = -Math.sin(p.yaw);
		const forwardZ = -Math.cos(p.yaw);
		const rightX = Math.cos(p.yaw);
		const rightZ = -Math.sin(p.yaw);

		const forwardSpeed = p.vx * forwardX + p.vz * forwardZ;
		const lateralSpeed = p.vx * rightX + p.vz * rightZ;

		let leftPower = input.drive + input.turn;
		let rightPower = input.drive - input.turn;
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
	}
}
