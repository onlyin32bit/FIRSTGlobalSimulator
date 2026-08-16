import type { FieldCollider } from './prediction';

export type AssimpNode = { name?: string; meshes?: number[]; transformation?: number[] };
export type AssimpScene = {
	rootnode?: { children?: AssimpNode[] };
	meshes?: Array<{ vertices?: number[] }>;
};

const length = (value: [number, number, number]) => Math.hypot(...value);

export function parseRobotColliders(scene: AssimpScene): FieldCollider[] {
	return (scene.rootnode?.children ?? []).flatMap((node) => {
		const meshIndex = node.meshes?.[0];
		const vertices = Number.isInteger(meshIndex) ? scene.meshes?.[meshIndex!]!.vertices : undefined;
		const matrix = node.transformation;
		if (!node.name || !vertices?.length || !matrix || matrix.length < 16) return [];

		const localMin = [Infinity, Infinity, Infinity];
		const localMax = [-Infinity, -Infinity, -Infinity];
		for (let index = 0; index < vertices.length; index += 3) {
			for (let axis = 0; axis < 3; axis += 1) {
				localMin[axis] = Math.min(localMin[axis]!, vertices[index + axis]!);
				localMax[axis] = Math.max(localMax[axis]!, vertices[index + axis]!);
			}
		}
		const localCenter = localMin.map((value, axis) => (value! + localMax[axis]!) * 0.5) as [
			number,
			number,
			number
		];
		const center: [number, number, number] = [
			matrix[0]! * localCenter[0] +
				matrix[1]! * localCenter[1] +
				matrix[2]! * localCenter[2] +
				matrix[3]!,
			matrix[4]! * localCenter[0] +
				matrix[5]! * localCenter[1] +
				matrix[6]! * localCenter[2] +
				matrix[7]!,
			matrix[8]! * localCenter[0] +
				matrix[9]! * localCenter[1] +
				matrix[10]! * localCenter[2] +
				matrix[11]!
		];
		const rawAxes: Array<[number, number, number]> = [
			[matrix[0]!, matrix[4]!, matrix[8]!],
			[matrix[1]!, matrix[5]!, matrix[9]!],
			[matrix[2]!, matrix[6]!, matrix[10]!]
		];
		const scales = rawAxes.map((axis) => Math.max(length(axis), 1e-6));
		const axes = rawAxes.map((axis, index) =>
			axis.map((value) => value / scales[index]!)
		) as FieldCollider['axes'];
		const halfExtents = localMin.map((value, axis) =>
			Math.max((localMax[axis]! - value!) * 0.5 * scales[axis]!, 0.01)
		) as [number, number, number];
		const min = [...center] as [number, number, number];
		const max = [...center] as [number, number, number];
		for (let worldAxis = 0; worldAxis < 3; worldAxis += 1) {
			const radius = axes.reduce(
				(sum, axis, localAxis) => sum + Math.abs(axis[worldAxis]!) * halfExtents[localAxis]!,
				0
			);
			min[worldAxis] -= radius;
			max[worldAxis] += radius;
		}
		return [{ id: node.name, min, max, center, halfExtents, axes }];
	});
}
