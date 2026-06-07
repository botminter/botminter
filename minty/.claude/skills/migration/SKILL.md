---
name: migration
description: >-
  Migrates existing permanent BotMinter workspaces to the ephemeral session
  model. Initializes shared bare clones from local workspace repos so that
  `bm start` can create sessions without a slow remote fetch. Use when the
  operator asks to "migrate workspaces", "set up shared clones",
  "transition to sessions", or "prepare for ephemeral sessions".
metadata:
  author: botminter
  version: 1.0.0
  category: migration
  tags: [migration, sessions, workspaces, shared-clones]
---

# Workspace Migration

Migrates existing permanent BotMinter workspaces to the ephemeral session model by initializing shared bare clones from local workspace project repos.

## Why Migrate

The session model replaces `bm teams sync` with on-demand ephemeral sessions created by `bm start`. Sessions use git worktrees from shared bare clones. Without migration, the first `bm start` clones each project repo from the remote — which can be slow for large repos. Migration pre-populates shared clones from existing local repos, making the first session instant.

Migration is an **optimization**, not a requirement. The daemon auto-initializes shared clones from remote on first session creation if they don't exist.

## What Migration Does

1. Discovers existing permanent workspaces (directories with `.botminter.workspace` marker)
2. For each workspace's project repos, creates a bare clone in the team's shared clones directory
3. Sets the remote URL on each bare clone to the upstream (so future fetches pull from the remote)
4. Leaves original workspace directories completely untouched — they serve as rollback fallback

## What Migration Does NOT Do

- Delete or modify permanent workspace directories
- Migrate any state or data — session state is new, not converted
- Require any manual steps after completion — `bm start` works immediately

## Running Migration

### Step 1: Discover Workspaces

Read `~/.botminter/config.yml` to find registered teams and their paths:

```bash
cat ~/.botminter/config.yml
```

For each team, scan the team directory for member workspaces:

```bash
TEAM_PATH="<team_path>"
for dir in "$TEAM_PATH"/*/; do
  if [ -f "$dir/.botminter.workspace" ]; then
    echo "Found workspace: $dir"
  fi
done
```

### Step 2: Identify Project Repos

For each workspace, list the project repos and their remote URLs:

```bash
WORKSPACE="<workspace_path>"
for project in "$WORKSPACE"/projects/*/; do
  if [ -d "$project/.git" ] || [ -f "$project/.git" ]; then
    REMOTE_URL=$(git -C "$project" remote get-url origin 2>/dev/null || echo "NO_REMOTE")
    echo "Project: $(basename "$project") -> $REMOTE_URL"
  fi
done
```

### Step 3: Create Shared Bare Clones

For each project repo, create a bare clone in the team's shared clones directory. The clones directory is at `<team_path>/.clones/`:

```bash
CLONES_DIR="$TEAM_PATH/.clones"
mkdir -p "$CLONES_DIR"

for project in "$WORKSPACE"/projects/*/; do
  if [ -d "$project/.git" ] || [ -f "$project/.git" ]; then
    REMOTE_URL=$(git -C "$project" remote get-url origin 2>/dev/null)
    if [ -z "$REMOTE_URL" ]; then
      echo "SKIP: $(basename "$project") — no remote configured"
      continue
    fi

    # Hash the URL to get the clone directory name (matches daemon convention)
    CLONE_NAME=$(echo -n "$REMOTE_URL" | sha256sum | cut -c1-16)
    CLONE_DIR="$CLONES_DIR/$CLONE_NAME"

    if [ -d "$CLONE_DIR" ]; then
      echo "SKIP: $CLONE_DIR already exists for $(basename "$project")"
      continue
    fi

    echo "Cloning $(basename "$project") from local workspace..."
    git clone --bare "$project" "$CLONE_DIR"

    # Set remote to upstream URL (local clone defaults to local path)
    git -C "$CLONE_DIR" remote set-url origin "$REMOTE_URL"

    echo "OK: $(basename "$project") -> $CLONE_DIR (remote: $REMOTE_URL)"
  fi
done
```

### Step 4: Verify

After migration, verify shared clones exist and have correct remotes:

```bash
for clone in "$CLONES_DIR"/*/; do
  if [ -d "$clone" ]; then
    REMOTE=$(git -C "$clone" remote get-url origin 2>/dev/null)
    echo "Clone: $(basename "$clone") -> $REMOTE"
  fi
done
```

Then test that `bm start` creates a session successfully:

```bash
bm start <member> -t <team>
bm status -t <team>
```

## Reporting

After migration, report to the operator:

```
## Migration Summary

| Workspace | Projects Found | Clones Created | Skipped |
|-----------|---------------|----------------|---------|
| member-a  | 3             | 3              | 0       |
| member-b  | 2             | 0              | 2 (already exist) |

Total: 5 project repos discovered, 3 new shared clones created.

Permanent workspaces preserved at their original paths.
Run `bm start` to create your first ephemeral session.
```

## Troubleshooting

### Clone Already Exists
If a shared clone already exists for a project URL, migration skips it. This is safe — the daemon manages clone freshness via fetch timestamps.

### No Remote URL
If a project repo has no remote configured, migration skips it. The project cannot be used in sessions without a remote URL.

### Permission Errors
Ensure the operator has write access to the team directory. Shared clones are created at `<team_path>/.clones/`.
