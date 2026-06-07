import type { TeamSummary, TeamOverview, ProcessData, MemberListEntry, MemberDetail, FileReadResponse, FileWriteResponse, TreeResponse, SyncResponse, ConsoleSessionSummary, ApiError } from './types.js';

class ApiClient {
	private baseUrl: string;

	constructor(baseUrl = '') {
		this.baseUrl = baseUrl;
	}

	private async request<T>(path: string, options?: RequestInit): Promise<T> {
		const response = await fetch(`${this.baseUrl}${path}`, options);
		if (!response.ok) {
			const body: ApiError = await response.json().catch(() => ({
				error: `HTTP ${response.status}: ${response.statusText}`
			}));
			throw new Error(body.error);
		}
		return response.json();
	}

	async fetchTeams(): Promise<TeamSummary[]> {
		return this.request<TeamSummary[]>('/api/teams');
	}

	async fetchOverview(team: string): Promise<TeamOverview> {
		return this.request<TeamOverview>(`/api/teams/${encodeURIComponent(team)}/overview`);
	}

	async fetchProcess(team: string): Promise<ProcessData> {
		return this.request<ProcessData>(`/api/teams/${encodeURIComponent(team)}/process`);
	}

	async fetchMembers(team: string): Promise<MemberListEntry[]> {
		return this.request<MemberListEntry[]>(`/api/teams/${encodeURIComponent(team)}/members`);
	}

	async fetchMember(team: string, name: string): Promise<MemberDetail> {
		return this.request<MemberDetail>(
			`/api/teams/${encodeURIComponent(team)}/members/${encodeURIComponent(name)}`
		);
	}

	async fetchFile(team: string, path: string): Promise<FileReadResponse> {
		return this.request<FileReadResponse>(
			`/api/teams/${encodeURIComponent(team)}/files/${path}`
		);
	}

	async saveFile(team: string, path: string, content: string): Promise<FileWriteResponse> {
		return this.request<FileWriteResponse>(
			`/api/teams/${encodeURIComponent(team)}/files/${path}`,
			{
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ content })
			}
		);
	}

	async fetchTree(team: string, path?: string): Promise<TreeResponse> {
		const params = path ? `?path=${encodeURIComponent(path)}` : '';
		return this.request<TreeResponse>(
			`/api/teams/${encodeURIComponent(team)}/tree${params}`
		);
	}

	async fetchSessions(team: string): Promise<ConsoleSessionSummary[]> {
		return this.request<ConsoleSessionSummary[]>(
			`/api/teams/${encodeURIComponent(team)}/sessions`
		);
	}

	async syncTeam(team: string): Promise<SyncResponse> {
		return this.request<SyncResponse>(
			`/api/teams/${encodeURIComponent(team)}/sync`,
			{ method: 'POST' }
		);
	}
}

export const api = new ApiClient();
