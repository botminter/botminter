import type { Page } from '@playwright/test';
import type {
	TeamSummary,
	TeamOverview,
	ProcessData,
	MemberListEntry,
	MemberDetail,
	TreeResponse
} from '../src/lib/types.js';

export const TEAM = 'my-team';
export const MEMBER = 'superman-alice';

export const mockTeams: TeamSummary[] = [
	{
		name: 'my-team',
		profile: 'scrum-compact',
		github_repo: 'myorg/my-team',
		path: '/home/user/.botminter/teams/my-team'
	}
];

export const mockOverview: TeamOverview = {
	name: 'my-team',
	profile: 'scrum-compact',
	display_name: 'Scrum Compact Solo Team',
	description: 'A compact single-member team',
	version: '1.0.0',
	github_repo: 'myorg/my-team',
	default_coding_agent: 'Claude Code',
	roles: [
		{ name: 'superman', description: 'All-in-one member' },
		{ name: 'team-manager', description: 'Process improvement' }
	],
	members: [
		{ name: 'superman-alice', role: 'superman', comment_emoji: '\u{1f9b8}', hat_count: 14 },
		{ name: 'team-manager-mgr', role: 'team-manager', comment_emoji: '\u{1f4cb}', hat_count: 1 }
	],
	status_count: 25,
	label_count: 4,
	projects: [{ name: 'my-app', fork_url: 'https://github.com/myorg/my-app' }],
	bridge: { selected: null, available: ['telegram', 'tuwunel', 'rocketchat'] },
	knowledge_files: ['commit-convention.md', 'pr-standards.md'],
	invariant_files: ['code-review-required.md', 'test-coverage.md']
};

export const mockProcess: ProcessData = {
	statuses: [
		{ name: 'po:triage', description: 'New issues awaiting triage' },
		{ name: 'arch:design', description: 'Architecture design phase' },
		{ name: 'dev:implement', description: 'Implementation in progress' }
	],
	workflows: [
		{ name: 'Story', dot: 'digraph { po_triage -> arch_design -> dev_implement }' }
	],
	labels: [
		{ name: 'bug', color: 'd73a4a', description: 'Something is broken' },
		{ name: 'enhancement', color: 'a2eeef', description: 'New feature or request' }
	],
	views: [
		{ name: 'All Work', prefixes: [], also_include: [] },
		{ name: 'Dev Board', prefixes: ['dev:'], also_include: [] }
	],
	markdown: '# Process\n\nThis is the team process document.'
};

export const mockMembers: MemberListEntry[] = [
	{
		name: 'superman-alice',
		role: 'superman',
		comment_emoji: '\u{1f9b8}',
		has_ralph_yml: true,
		hat_count: 14
	},
	{
		name: 'team-manager-mgr',
		role: 'team-manager',
		comment_emoji: '\u{1f4cb}',
		has_ralph_yml: true,
		hat_count: 1
	}
];

export const mockMemberDetail: MemberDetail = {
	name: 'superman-alice',
	role: 'superman',
	comment_emoji: '\u{1f9b8}',
	ralph_yml:
		'hats:\n  po_backlog:\n    name: Backlog Manager\n    description: Handles backlog\n',
	claude_md: '# Superman Context\n\nThis is the CLAUDE.md content.',
	prompt_md: '# Objective\n\nAdvance all GitHub issues.',
	hats: [
		{
			name: 'po_backlog',
			description: 'Handles backlog',
			triggers: ['po.backlog'],
			publishes: ['po.backlog.failed']
		},
		{
			name: 'dev_implementer',
			description: 'Implements stories',
			triggers: ['dev.implement'],
			publishes: ['dev.done']
		}
	],
	knowledge_files: ['commit-convention.md'],
	invariant_files: ['design-quality.md'],
	skill_dirs: ['gh', 'board-scanner']
};

export const mockTree: TreeResponse = {
	path: '',
	entries: [
		{ name: 'knowledge', type: 'directory', path: 'knowledge' },
		{ name: 'invariants', type: 'directory', path: 'invariants' },
		{ name: 'members', type: 'directory', path: 'members' },
		{ name: 'PROCESS.md', type: 'file', path: 'PROCESS.md' },
		{ name: 'botminter.yml', type: 'file', path: 'botminter.yml' }
	]
};

/**
 * Sets up API route interception on a Playwright page.
 * All /api/* requests are intercepted and return mock data,
 * allowing tests to verify DOM rendering without a live backend.
 */
export async function mockApi(page: Page): Promise<void> {
	await page.route('**/api/teams', (route) => {
		if (route.request().method() === 'GET') {
			return route.fulfill({ json: mockTeams });
		}
		return route.fallback();
	});

	await page.route(`**/api/teams/${TEAM}/overview`, (route) =>
		route.fulfill({ json: mockOverview })
	);

	await page.route(`**/api/teams/${TEAM}/process`, (route) =>
		route.fulfill({ json: mockProcess })
	);

	await page.route(`**/api/teams/${TEAM}/members/${MEMBER}`, (route) =>
		route.fulfill({ json: mockMemberDetail })
	);

	await page.route(`**/api/teams/${TEAM}/members`, (route) =>
		route.fulfill({ json: mockMembers })
	);

	await page.route(`**/api/teams/${TEAM}/tree**`, (route) =>
		route.fulfill({ json: mockTree })
	);

	await page.route(`**/api/teams/${TEAM}/files/**`, (route) =>
		route.fulfill({
			json: {
				path: 'PROCESS.md',
				content: '# Process\n\nTeam process document.',
				content_type: 'markdown',
				last_modified: '2026-01-01T00:00:00Z'
			}
		})
	);
}
