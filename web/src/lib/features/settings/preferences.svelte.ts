import { browser } from '$app/environment';

export type DriveMode = 'arcade-left' | 'arcade-right' | 'split-arcade' | 'tank';

export type UserPreferences = {
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
	};
};

export const defaultPreferences = (): UserPreferences => ({
	graphics: {
		quality: 'high',
		shadows: true,
		antiAliasing: true,
		showEffects: true,
		resolutionScale: 100,
		cameraFov: 50
	},
	controls: {
		driveMode: 'arcade-left',
		intakeButton: 4,
		outtakeButton: 5
	}
});

export function preferencesKey(userId: string) {
	return `fgsim:preferences:${userId}`;
}

export function loadPreferences(userId: string) {
	if (!browser) return defaultPreferences();
	try {
		const saved = JSON.parse(localStorage.getItem(preferencesKey(userId)) ?? 'null');
		return saved
			? {
					...defaultPreferences(),
					...saved,
					graphics: { ...defaultPreferences().graphics, ...saved.graphics },
					controls: { ...defaultPreferences().controls, ...saved.controls }
				}
			: defaultPreferences();
	} catch {
		return defaultPreferences();
	}
}

export function savePreferences(userId: string, preferences: UserPreferences) {
	if (browser) localStorage.setItem(preferencesKey(userId), JSON.stringify(preferences));
}
