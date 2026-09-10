import { browser } from '$app/environment';

export type DriveMode = 'arcade-left' | 'arcade-right' | 'split-arcade' | 'tank';

export type UserPreferences = {
	version?: number;
	graphics: {
		quality: 'low' | 'medium' | 'high';
		shadows: boolean;
		antiAliasing: boolean;
		showEffects: boolean;
		resolutionScale: number;
		cameraFov: number;
	};
	controls: {
		driveMode: DriveMode;
		intakeButton: number;
		outtakeButton: number;
		climbButton: number;
	};
};

export const defaultPreferences = (): UserPreferences => ({
	version: 2,
	graphics: {
		quality: 'high',
		shadows: true,
		antiAliasing: true,
		showEffects: true,
		resolutionScale: 100,
		cameraFov: 50
	},
	controls: {
		driveMode: 'split-arcade',
		intakeButton: 4,
		outtakeButton: 5,
		climbButton: 3
	}
});

export function preferencesKey(userId: string) {
	return `fgsim:preferences:${userId}`;
}

export function loadPreferences(userId: string): UserPreferences {
	if (!browser) return defaultPreferences();
	try {
		const saved = JSON.parse(localStorage.getItem(preferencesKey(userId)) ?? 'null');
		if (!saved) return defaultPreferences();
		const merged: UserPreferences = {
			...defaultPreferences(),
			...saved,
			graphics: { ...defaultPreferences().graphics, ...saved.graphics },
			controls: { ...defaultPreferences().controls, ...saved.controls }
		};
		// Migrate legacy preferences without version or version < 2 that had old default 'arcade-left'
		if (!saved.version || saved.version < 2) {
			if (merged.controls.driveMode === 'arcade-left') {
				merged.controls.driveMode = 'split-arcade';
			}
			merged.version = 2;
			savePreferences(userId, merged);
		}
		return merged;
	} catch {
		return defaultPreferences();
	}
}

export function savePreferences(userId: string, preferences: UserPreferences) {
	if (browser) localStorage.setItem(preferencesKey(userId), JSON.stringify(preferences));
}
