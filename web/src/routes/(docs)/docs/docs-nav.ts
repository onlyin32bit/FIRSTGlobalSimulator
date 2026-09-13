import type { Pathname } from '$app/types';

export type DocsNavItem = {
	title: string;
	href: Pathname;
	description: string;
	headings: Array<{ id: string; title: string }>;
};

export type DocsNavSection = {
	title: string;
	items: DocsNavItem[];
};

export const docsNav: DocsNavSection[] = [
	{
		title: 'Start here',
		items: [
			{
				title: 'Overview',
				href: '/docs',
				description: 'The simulator at a glance.',
				headings: [
					{ id: 'the-complete-loop', title: 'The complete loop' },
					{ id: 'simulation-contract', title: 'Simulation contract' }
				]
			},
			{
				title: 'Getting started',
				href: '/docs/getting-started',
				description: 'Accounts, dashboard, and first match.',
				headings: [
					{ id: 'access-and-account', title: 'Access and account' },
					{ id: 'start-a-match', title: 'Start a match' },
					{ id: 'join-and-rejoin', title: 'Join and rejoin' }
				]
			},
			{
				title: 'Match play',
				href: '/docs/match-play',
				description: 'Lobby roles, lifecycle, and results.',
				headings: [
					{ id: 'lobby-and-roles', title: 'Lobby and roles' },
					{ id: 'match-lifecycle', title: 'Match lifecycle' },
					{ id: 'after-the-buzzer', title: 'After the buzzer' }
				]
			},
			{
				title: 'Controls & cameras',
				href: '/docs/controls',
				description: 'Keyboard, gamepad, cameras, and diagnostics.',
				headings: [
					{ id: 'drive-and-mechanisms', title: 'Drive and mechanisms' },
					{ id: 'camera-workflow', title: 'Camera workflow' },
					{ id: 'diagnostics', title: 'Diagnostics' }
				]
			}
		]
	},
	{
		title: 'Game guide',
		items: [
			{
				title: 'FGC 2026 rules',
				href: '/docs/game-rules',
				description: 'Objects, scoring, climbing, and field behavior.',
				headings: [
					{ id: 'match-objective', title: 'Match objective' },
					{ id: 'scoring-and-climbing', title: 'Scoring and climbing' },
					{ id: 'field-authority', title: 'Field authority' }
				]
			},
			{
				title: 'Robot & mechanisms',
				href: '/docs/robot-builder',
				description: 'Starter Bot runtime, ball flow, and revisions.',
				headings: [
					{ id: 'starter-bot', title: 'Starter Bot' },
					{ id: 'ball-path', title: 'Ball path' },
					{ id: 'robot-revisions', title: 'Robot revisions' }
				]
			},
			{
				title: 'Simulator',
				href: '/docs/simulator',
				description: 'Authority, networking, and performance model.',
				headings: [
					{ id: 'authoritative-match-service', title: 'Authoritative match service' },
					{ id: 'smooth-networked-play', title: 'Smooth networked play' },
					{ id: 'sandbox-and-review', title: 'Sandbox and review' }
				]
			}
		]
	},
	{
		title: 'Operations',
		items: [
			{
				title: 'Administration',
				href: '/docs/administration',
				description: 'Host controls, recovery, and room safety.',
				headings: [
					{ id: 'authority-and-safety', title: 'Authority and safety' },
					{ id: 'live-match-actions', title: 'Live match actions' },
					{ id: 'operating-a-room', title: 'Operating a room' }
				]
			},
			{
				title: 'FAQ',
				href: '/docs/faq',
				description: 'Fast answers and troubleshooting.',
				headings: [
					{ id: 'joining-and-controls', title: 'Joining and controls' },
					{ id: 'match-and-scoring', title: 'Match and scoring' },
					{ id: 'support', title: 'Support' }
				]
			}
		]
	}
];

export const docsItems = docsNav.flatMap((section) => section.items);

export function activeDocsItem(pathname: string) {
	return docsItems.find((item) => item.href === pathname) ?? docsItems[0];
}
