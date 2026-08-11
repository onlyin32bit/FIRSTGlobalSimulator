export type TopDownLayout = {
	worldBounds: { min: [number, number]; max: [number, number] };
	horizontalDirection: 'inverted' | 'normal';
	verticalDirection: 'inverted' | 'normal';
};

export class FieldProjector {
	constructor(private readonly layout: TopDownLayout) {}

	project(point: [number, number, number]) {
		const { min, max } = this.layout.worldBounds;
		const x = ((point[0] - min[0]) / (max[0] - min[0])) * 100;
		const z = ((point[2] - min[1]) / (max[1] - min[1])) * 100;
		return {
			left: this.layout.horizontalDirection === 'inverted' ? 100 - x : x,
			top: this.layout.verticalDirection === 'inverted' ? 100 - z : z
		};
	}

	projectBounds(bounds: { min: [number, number, number]; max: [number, number, number] }) {
		const first = this.project(bounds.min);
		const second = this.project(bounds.max);
		return {
			left: Math.min(first.left, second.left),
			top: Math.min(first.top, second.top),
			width: Math.abs(second.left - first.left),
			height: Math.abs(second.top - first.top)
		};
	}

	projectFootprint(center: [number, number, number], widthM: number, heightM: number) {
		return this.projectBounds({
			min: [center[0] - widthM / 2, center[1], center[2] - heightM / 2],
			max: [center[0] + widthM / 2, center[1], center[2] + heightM / 2]
		});
	}
}
