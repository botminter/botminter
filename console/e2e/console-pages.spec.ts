import { test, expect } from '@playwright/test';
import { TEAM, MEMBER, mockApi } from './fixtures.js';

// Scope assertions to <main> to avoid conflicts with sidebar navigation text
const main = (page: import('@playwright/test').Page) => page.locator('main');

test.describe('Root page', () => {
	test('loads and redirects to team overview', async ({ page }) => {
		await mockApi(page);
		await page.goto('/');
		await expect(page).toHaveURL(new RegExp(`/teams/${TEAM}/overview`));
	});
});

test.describe('Overview page', () => {
	test.beforeEach(async ({ page }) => {
		await mockApi(page);
		await page.goto(`/teams/${TEAM}/overview`);
	});

	test('renders team name and description', async ({ page }) => {
		const content = main(page);
		await expect(content.getByRole('heading', { name: 'my-team' })).toBeVisible();
		await expect(content.getByText('A compact single-member team')).toBeVisible();
	});

	test('renders profile and coding agent', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('scrum-compact').first()).toBeVisible();
		await expect(content.getByText('Coding Agent: Claude Code')).toBeVisible();
	});

	test('renders roles section', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('2 defined')).toBeVisible();
		await expect(content.getByText('All-in-one member').first()).toBeVisible();
	});

	test('renders members section with hat counts', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('superman-alice')).toBeVisible();
		await expect(content.getByText('14 hats')).toBeVisible();
		await expect(content.getByText('team-manager-mgr')).toBeVisible();
		await expect(content.getByText('1 hat')).toBeVisible();
	});

	test('renders knowledge and invariant files', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('commit-convention.md')).toBeVisible();
		await expect(content.getByText('test-coverage.md')).toBeVisible();
	});

	test('renders project info', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('my-app').first()).toBeVisible();
		await expect(content.getByText('1 configured')).toBeVisible();
	});

	test('renders bridge status', async ({ page }) => {
		await expect(main(page).getByText('Not configured')).toBeVisible();
	});
});

test.describe('Members list page', () => {
	test.beforeEach(async ({ page }) => {
		await mockApi(page);
		await page.goto(`/teams/${TEAM}/members`);
	});

	test('renders member cards with names', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('superman-alice')).toBeVisible();
		await expect(content.getByText('team-manager-mgr')).toBeVisible();
	});

	test('shows member count', async ({ page }) => {
		await expect(main(page).getByText('2 members')).toBeVisible();
	});

	test('shows hat counts', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('14 hats')).toBeVisible();
		await expect(content.getByText('1 hat')).toBeVisible();
	});

	test('member cards link to detail page', async ({ page }) => {
		const link = main(page).getByText('superman-alice').locator('xpath=ancestor::a');
		await expect(link).toHaveAttribute('href', `/teams/${TEAM}/members/superman-alice`);
	});
});

test.describe('Member detail page', () => {
	test.beforeEach(async ({ page }) => {
		await mockApi(page);
		await page.goto(`/teams/${TEAM}/members/${MEMBER}`);
	});

	test('renders member name and role badge', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('superman-alice')).toBeVisible();
		// Role badge uses exact text
		await expect(content.getByText('superman', { exact: true }).first()).toBeVisible();
	});

	test('renders tab buttons', async ({ page }) => {
		const content = main(page);
		await expect(content.getByRole('button', { name: 'Ralph YAML' })).toBeVisible();
		await expect(content.getByRole('button', { name: 'CLAUDE.md' })).toBeVisible();
		await expect(content.getByRole('button', { name: 'PROMPT.md' })).toBeVisible();
		await expect(content.getByRole('button', { name: 'Hats' })).toBeVisible();
	});

	test('switches to CLAUDE.md tab and shows content', async ({ page }) => {
		await main(page).getByRole('button', { name: 'CLAUDE.md' }).click();
		await expect(main(page).getByText('Superman Context')).toBeVisible();
	});

	test('switches to PROMPT.md tab and shows content', async ({ page }) => {
		await main(page).getByRole('button', { name: 'PROMPT.md' }).click();
		await expect(main(page).getByText('Objective')).toBeVisible();
	});

	test('switches to Hats tab and shows hat details', async ({ page }) => {
		await main(page).getByRole('button', { name: 'Hats' }).click();
		await expect(main(page).getByText('po_backlog')).toBeVisible();
		await expect(main(page).getByText('Handles backlog')).toBeVisible();
		await expect(main(page).getByText('dev_implementer')).toBeVisible();
	});

	test('has back link to members list', async ({ page }) => {
		await expect(main(page).getByText('← Members')).toBeVisible();
	});
});

test.describe('Process page', () => {
	test.beforeEach(async ({ page }) => {
		await mockApi(page);
		await page.goto(`/teams/${TEAM}/process`);
	});

	test('renders status table with entries', async ({ page }) => {
		const content = main(page);
		// Click Statuses tab first to ensure table is visible
		await content.getByRole('button', { name: 'Statuses' }).click();
		await expect(content.getByText('po:triage')).toBeVisible();
		await expect(content.getByText('arch:design')).toBeVisible();
		await expect(content.getByText('dev:implement')).toBeVisible();
	});

	test('renders tab navigation', async ({ page }) => {
		const content = main(page);
		await expect(content.getByRole('button', { name: 'Statuses' })).toBeVisible();
		await expect(content.getByRole('button', { name: 'Labels' })).toBeVisible();
		await expect(content.getByRole('button', { name: 'Views' })).toBeVisible();
	});

	test('shows labels on Labels tab', async ({ page }) => {
		const content = main(page);
		await content.getByRole('button', { name: 'Labels' }).click();
		await expect(content.getByText('bug')).toBeVisible();
		await expect(content.getByText('enhancement')).toBeVisible();
	});

	test('shows views on Views tab', async ({ page }) => {
		const content = main(page);
		await content.getByRole('button', { name: 'Views' }).click();
		await expect(content.getByText('All Work')).toBeVisible();
		await expect(content.getByText('Dev Board')).toBeVisible();
	});

	test('shows markdown on PROCESS.md tab', async ({ page }) => {
		const content = main(page);
		await content.getByRole('button', { name: 'PROCESS.md' }).click();
		await expect(content.getByText('This is the team process document.')).toBeVisible();
	});
});

test.describe('Files browser page', () => {
	test.beforeEach(async ({ page }) => {
		await mockApi(page);
		await page.goto(`/teams/${TEAM}/files/`);
	});

	test('renders file tree entries', async ({ page }) => {
		const content = main(page);
		// Use more specific selectors — file tree entries are inside the main content
		await expect(content.locator('a[href*="/files/knowledge"]')).toBeVisible();
		await expect(content.locator('a[href*="/files/invariants"]')).toBeVisible();
		await expect(content.locator('a[href*="/files/members"]')).toBeVisible();
	});

	test('renders files distinct from directories', async ({ page }) => {
		const content = main(page);
		await expect(content.getByText('PROCESS.md')).toBeVisible();
		await expect(content.getByText('botminter.yml')).toBeVisible();
	});

	test('directory entries are clickable links', async ({ page }) => {
		const content = main(page);
		const knowledgeLink = content.locator('a[href*="/files/knowledge"]');
		await expect(knowledgeLink).toBeVisible();
		await expect(knowledgeLink).toHaveAttribute('href', `/teams/${TEAM}/files/knowledge`);
	});
});
