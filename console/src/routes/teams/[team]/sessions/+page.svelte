<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import type { ConsoleSessionSummary } from '$lib/types.js';
	import { api } from '$lib/api.js';

	const team = $derived($page.params.team ?? '');
	let sessions = $state<ConsoleSessionSummary[]>([]);
	let error = $state<string | null>(null);
	let loading = $state(true);

	onMount(async () => {
		try {
			sessions = await api.fetchSessions(team);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load sessions';
		} finally {
			loading = false;
		}
	});

	function stateBadgeClass(state: string): string {
		switch (state.toLowerCase()) {
			case 'active':
				return 'bg-green-100 text-green-700 border border-green-300';
			case 'finalizing':
				return 'bg-yellow-100 text-yellow-700 border border-yellow-300';
			case 'completed':
				return 'bg-gray-100 text-gray-600 border border-gray-300';
			case 'failed':
				return 'bg-red-100 text-red-700 border border-red-300';
			case 'killed':
				return 'bg-orange-100 text-orange-700 border border-orange-300';
			case 'retained':
				return 'bg-blue-100 text-blue-700 border border-blue-300';
			default:
				return 'bg-gray-100 text-gray-600 border border-gray-300';
		}
	}
</script>

<header class="border-b border-surface-border px-8 py-5">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-xl font-semibold text-gray-900">Sessions</h1>
			<p class="text-sm text-gray-500 mt-0.5">Active and recent member sessions</p>
		</div>
		{#if !loading && !error}
			<span class="text-xs text-gray-500">{sessions.length} {sessions.length === 1 ? 'session' : 'sessions'}</span>
		{/if}
	</div>
</header>

{#if loading}
	<div class="p-8">
		<p class="text-gray-500">Loading...</p>
	</div>
{:else if error}
	<div class="p-8">
		<div class="bg-red-500/10 border border-red-500/20 rounded-md p-4 text-red-400 text-sm">
			{error}
		</div>
	</div>
{:else}
	<div class="p-8">
		{#if sessions.length === 0}
			<div class="bg-surface-raised border border-surface-border rounded-lg p-8 text-center">
				<p class="text-gray-500">No sessions found.</p>
				<p class="text-gray-500 text-sm mt-1">Sessions appear when members are running.</p>
			</div>
		{:else}
			<div class="bg-surface-raised border border-surface-border rounded-lg overflow-hidden">
				<table class="w-full text-sm">
					<thead>
						<tr class="border-b border-surface-border">
							<th class="text-left px-4 py-3 text-xs font-medium text-gray-500 uppercase tracking-wider">Member</th>
							<th class="text-left px-4 py-3 text-xs font-medium text-gray-500 uppercase tracking-wider">State</th>
							<th class="text-left px-4 py-3 text-xs font-medium text-gray-500 uppercase tracking-wider">Type</th>
							<th class="text-left px-4 py-3 text-xs font-medium text-gray-500 uppercase tracking-wider">Created</th>
							<th class="text-left px-4 py-3 text-xs font-medium text-gray-500 uppercase tracking-wider">Finalization</th>
						</tr>
					</thead>
					<tbody>
						{#each sessions as session}
							<tr class="border-b border-surface-border last:border-0 hover:bg-black/5">
								<td class="px-4 py-3 font-medium text-gray-900">{session.member_name}</td>
								<td class="px-4 py-3">
									<span class="text-xs px-2 py-0.5 rounded font-medium {stateBadgeClass(session.state)}">
										{session.state}
									</span>
								</td>
								<td class="px-4 py-3 text-gray-600">{session.session_type}</td>
								<td class="px-4 py-3 text-gray-500">{new Date(session.created_at).toLocaleString()}</td>
								<td class="px-4 py-3 text-gray-500">{session.finalization_status}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</div>
{/if}
