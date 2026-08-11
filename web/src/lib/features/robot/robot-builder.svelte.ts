import { api, ApiError, type Robot } from '$lib/api';

export const robotOptions = {
	driveType: ['Mecanum', 'Tank', 'Swerve', 'H-Drive'],
	intake: ['Roller claw', 'Over-the-bumper', 'None'],
	shooter: ['Flywheel', 'Catapult', 'Puncher', 'None']
} as const;

export class RobotBuilder {
	name = $state('My robot');
	driveType = $state<(typeof robotOptions.driveType)[number]>('Mecanum');
	intake = $state<(typeof robotOptions.intake)[number]>('Roller claw');
	shooter = $state<(typeof robotOptions.shooter)[number]>('Flywheel');
	robots = $state<Robot[]>([]);
	isLoading = $state(true);
	isSaving = $state(false);
	message = $state('');

	get summary() {
		return `${this.driveType} drive · ${this.intake} intake · ${this.shooter} scorer`;
	}

	async load() {
		this.isLoading = true;
		try {
			this.robots = (await api.listRobots()).robots;
		} catch (error) {
			this.message = error instanceof ApiError ? error.message : 'Could not load saved robots.';
		} finally {
			this.isLoading = false;
		}
	}

	async save() {
		this.isSaving = true;
		this.message = '';
		try {
			const { robot } = await api.createRobot({
				name: this.name,
				buildData: { driveType: this.driveType, intake: this.intake, shooter: this.shooter }
			});
			this.robots = [robot, ...this.robots];
			this.message = `${robot.name} saved.`;
		} catch (error) {
			this.message = error instanceof ApiError ? error.message : 'Could not save this robot.';
		} finally {
			this.isSaving = false;
		}
	}
}
