<script lang="ts">
	import { page } from '$app/stores';
	import type { TeamSummary } from '$lib/types.js';
	import TeamSelector from './TeamSelector.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	interface Props {
		teams: TeamSummary[];
		team: string;
	}

	let { teams, team }: Props = $props();

	interface NavItem {
		label: string;
		href: string;
		icon: string;
	}

	const navItems: NavItem[] = [
		{
			label: 'Overview',
			href: 'overview',
			icon: 'M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4'
		},
		{
			label: 'Process',
			href: 'process',
			icon: 'M9 17V7m0 10a2 2 0 01-2 2H5a2 2 0 01-2-2V7a2 2 0 012-2h2a2 2 0 012 2m0 10a2 2 0 002 2h2a2 2 0 002-2M9 7a2 2 0 012-2h2a2 2 0 012 2m0 10V7m0 10a2 2 0 002 2h2a2 2 0 002-2V7a2 2 0 00-2-2h-2a2 2 0 00-2 2'
		},
		{
			label: 'Members',
			href: 'members',
			icon: 'M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z'
		},
		{
			label: 'Sessions',
			href: 'sessions',
			icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2'
		},
		{
			label: 'Files',
			href: 'files',
			icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
		},
		{
			label: 'Knowledge',
			href: 'knowledge',
			icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253'
		},
		{
			label: 'Invariants',
			href: 'invariants',
			icon: 'M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z'
		},
		{
			label: 'Settings',
			href: 'settings',
			icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z'
		}
	];

	function isActive(href: string, pathname: string): boolean {
		const teamBase = `/teams/${team}`;
		return pathname.startsWith(`${teamBase}/${href}`);
	}
</script>

<aside class="w-56 bg-surface-raised border-r border-surface-border flex flex-col min-h-screen sticky top-0">
	<div class="p-4 border-b border-surface-border">
		<div class="flex items-center gap-2 mb-1">
			<img src="/logo.png" alt="BotMinter" class="h-7 w-auto" />
			<span class="font-semibold text-sm text-gray-900">BotMinter</span>
		</div>
		<div class="text-xs text-gray-500 ml-9">Console</div>
	</div>

	<div class="px-3 py-3 border-b border-surface-border">
		<TeamSelector {teams} selected={team} />
	</div>

	<nav class="flex-1 py-3" aria-label="Main navigation">
		{#each navItems as item (item.label)}
			{@const active = isActive(item.href, $page.url.pathname)}
			<a
				href="/teams/{team}/{item.href}"
				class="flex items-center gap-3 px-4 py-2 text-sm {active
					? 'bg-accent/10 text-accent border-r-2 border-accent'
					: 'text-gray-500 hover:text-gray-900 hover:bg-black/5'}"
				aria-current={active ? 'page' : undefined}
			>
				<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d={item.icon} />
				</svg>
				{item.label}
			</a>
		{/each}
	</nav>

	<div class="border-t border-surface-border">
		<ThemeToggle />
	</div>
</aside>
