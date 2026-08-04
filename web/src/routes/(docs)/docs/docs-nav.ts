export type DocsNavItem = {
	title: string;
	href: string;
	description: string;
	headings: Array<{ id: string; title: string }>;
};

export type DocsNavSection = {
	title: string;
	items: DocsNavItem[];
};

export const docsNav: DocsNavSection[] = [
	{
		title: 'User guide',
		items: [
			{
				title: 'Overview',
				href: '/docs',
				description: 'What the simulator offers today.',
				headings: [
					{ id: 'what-you-can-do', title: 'What you can do' },
					{ id: 'choose-your-path', title: 'Choose your path' }
				]
			},
			{
				title: 'Getting started',
				href: '/docs/getting-started',
				description: 'Access, registration, and dashboard basics.',
				headings: [
					{ id: 'request-access', title: 'Request access' },
					{ id: 'create-your-account', title: 'Create your account' },
					{ id: 'open-the-dashboard', title: 'Open the dashboard' }
				]
			},
			{
				title: 'Robot builder',
				href: '/docs/robot-builder',
				description: 'Configure and save your robot build.',
				headings: [
					{ id: 'configure-a-build', title: 'Configure a build' },
					{ id: 'save-your-robot', title: 'Save your robot' },
					{ id: 'saved-builds', title: 'Saved builds' }
				]
			},
			{
				title: 'Simulator',
				href: '/docs/simulator',
				description: 'Dashboard, lobbies, sandbox, and status.',
				headings: [
					{ id: 'create-a-lobby', title: 'Create a lobby' },
					{ id: 'offline-sandbox', title: 'Offline sandbox' },
					{ id: 'current-limitations', title: 'Current limitations' }
				]
			},
			{
				title: 'FAQ',
				href: '/docs/faq',
				description: 'Answers for access and account issues.',
				headings: [
					{ id: 'access-and-invitations', title: 'Access and invitations' },
					{ id: 'account-and-session', title: 'Account and session' },
					{ id: 'need-help', title: 'Need help?' }
				]
			}
		]
	}
];

export const docsItems = docsNav.flatMap((section) => section.items);

export function activeDocsItem(pathname: string) {
	return docsItems.find((item) => item.href === pathname) ?? docsItems[0];
}
