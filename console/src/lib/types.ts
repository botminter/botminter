export interface TeamSummary {
	name: string;
	profile: string;
	github_repo: string;
	path: string;
}

export interface TeamOverview {
	name: string;
	profile: string;
	display_name: string;
	description: string;
	version: string;
	github_repo: string;
	default_coding_agent: string | null;
	roles: RoleSummary[];
	members: MemberSummary[];
	status_count: number;
	label_count: number;
	projects: ProjectSummary[];
	bridge: BridgeOverview;
	knowledge_files: string[];
	invariant_files: string[];
}

export interface RoleSummary {
	name: string;
	description: string;
}

export interface MemberSummary {
	name: string;
	role: string;
	comment_emoji: string;
	hat_count: number;
}

export interface ProjectSummary {
	name: string;
	fork_url: string;
}

export interface BridgeOverview {
	selected: string | null;
	available: string[];
}

export interface MemberListEntry {
	name: string;
	role: string;
	comment_emoji: string;
	has_ralph_yml: boolean;
	hat_count: number;
}

export interface MemberDetail {
	name: string;
	role: string;
	comment_emoji: string;
	ralph_yml: string | null;
	claude_md: string | null;
	prompt_md: string | null;
	hats: HatSummary[];
	knowledge_files: string[];
	invariant_files: string[];
	skill_dirs: string[];
}

export interface HatSummary {
	name: string;
	description: string;
	triggers: string[];
	publishes: string[];
}

export interface FileReadResponse {
	path: string;
	content: string;
	content_type: 'yaml' | 'markdown' | 'json' | 'text';
	last_modified: string;
}

export interface FileWriteResponse {
	ok: boolean;
	path: string;
	commit_sha: string;
}

export interface TreeEntry {
	name: string;
	type: 'file' | 'directory';
	path: string;
}

export interface TreeResponse {
	path: string;
	entries: TreeEntry[];
}

export interface ProcessData {
	markdown: string | null;
	workflows: WorkflowEntry[];
	statuses: StatusEntry[];
	labels: LabelEntry[];
	views: ViewEntry[];
}

export interface WorkflowEntry {
	name: string;
	dot: string;
}

export interface StatusEntry {
	name: string;
	description: string;
}

export interface LabelEntry {
	name: string;
	color: string;
	description: string;
}

export interface ViewEntry {
	name: string;
	prefixes: string[];
	also_include: string[];
}

export interface SyncResponse {
	ok: boolean;
	message: string;
	changed_files: string[];
}

export interface ConsoleSessionSummary {
	session_id: string;
	member_name: string;
	state: string;
	session_type: string;
	created_at: string;
	finalization_status: string;
}

export interface ApiError {
	error: string;
}
