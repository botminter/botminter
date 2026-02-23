# Nono: Trust Verification & Instruction File Signing

> How nono verifies instruction files (CLAUDE.md, SKILLS.md, etc.) before letting an AI agent read them, and the full signing workflow to make it work.

## TL;DR

**Nono blocks instruction files by default.** When you run `nono run`, it scans the working directory for files matching patterns like `CLAUDE*`, `SKILLS*`, `AGENT*`, and `.claude/**/*.md`. Each match must have a cryptographically signed `.bundle` sidecar. If not, the default `enforcement=deny` mode hard-blocks execution. The only quick escape is `--trust-override`.

---

## The Default Behavior That Bites You

When no `trust-policy.json` exists anywhere (no project-level, no user-level), nono uses a built-in default policy:

```json
{
  "version": 1,
  "instruction_patterns": ["SKILLS*", "CLAUDE*", "AGENT*", ".claude/**/*.md"],
  "publishers": [],
  "blocklist": { "digests": [], "publishers": [] },
  "enforcement": "deny"
}
```

This means:

1. Any `CLAUDE.md` in your project root gets detected
2. No publishers are trusted (empty list)
3. No `.bundle` file exists next to it
4. Outcome: `Unsigned` + `enforcement=deny` = **hard block**

```
  Scanning 1 instruction file(s) for trust verification...
    FAIL CLAUDE.md (no .bundle file)

  Trust scan: 0 verified, 1 blocked, 0 warned
  Aborting: instruction files failed trust verification (enforcement=deny).
```

### Quick fix

Pass `--trust-override` to skip the entire trust scan:

```bash
nono run --trust-override --allow . -- claude
```

This logs a warning but lets execution proceed. It's a per-invocation flag — you need it every time.

---

## Enforcement Modes

| Mode | Value | Behavior |
|------|-------|----------|
| `audit` | 0 | Silent allow, log for post-hoc review |
| `warn` | 1 | Log warning, allow execution |
| `deny` | 2 | **Hard block** (default) |

When multiple policies are merged (user + project), the **strictest** mode wins. A project-level policy cannot weaken a user-level enforcement setting.

Blocklisted files (by SHA-256 digest) are **always blocked** regardless of enforcement mode — even in `audit`.

---

## The Full Signing Bootstrap

To use trust verification properly (no `--trust-override`), you need to set up the entire signing chain. The steps are ordered — each depends on the previous.

### Step 1: Generate a signing key

```bash
nono trust keygen
```

This creates an ECDSA P-256 key pair and stores it in the system keystore:
- Private key: `nono-trust` service, account `default`
- Public key: `nono-trust-pub` service, account `default`

The output includes a base64 public key. **Save this** — you need it for the trust policy.

To use a custom key ID:

```bash
nono trust keygen --id my-signing-key
```

### Step 2: Create a trust policy

Create `trust-policy.json` in the project root or `~/.config/nono/trust-policy.json` for user-level:

```json
{
  "version": 1,
  "instruction_patterns": ["SKILLS*", "CLAUDE*", "AGENT*", ".claude/**/*.md"],
  "publishers": [
    {
      "name": "local-dev",
      "key_id": "nono-keystore:default",
      "public_key": "<base64 public key from step 1>"
    }
  ],
  "blocklist": { "digests": [], "publishers": [] },
  "enforcement": "deny"
}
```

### Step 3: Sign the trust policy

```bash
nono trust sign-policy
```

This reads `trust-policy.json` from CWD, signs it with the `default` key, and produces `trust-policy.json.bundle`. You can specify a different key:

```bash
nono trust sign-policy --key my-signing-key
```

Or a different file:

```bash
nono trust sign-policy path/to/trust-policy.json
```

**The policy itself must be signed.** During `nono run`, the pre-exec scan calls `verify_policy_signature()` which requires the `.bundle` sidecar. An unsigned policy is a hard error (unless `--trust-override`).

However, during `nono trust verify` and `nono trust list`, unsigned policies only produce a warning and still load. This asymmetry supports development workflows.

### Step 4: Sign instruction files

```bash
# Sign a specific file
nono trust sign CLAUDE.md

# Sign all files matching the policy patterns in CWD
nono trust sign --all

# Sign with a specific key
nono trust sign CLAUDE.md --key my-signing-key
```

Each signed file gets a `.bundle` sidecar (e.g., `CLAUDE.md.bundle`).

### Step 5: Verify (optional, for checking)

```bash
# Verify specific files
nono trust verify CLAUDE.md

# Verify all instruction files in CWD
nono trust verify --all

# List all instruction files with status
nono trust list
```

---

## Per-Project Signing

`nono trust sign --all` is **relative to CWD**. The `find_instruction_files` function walks the directory tree recursively (up to 16 levels, skipping hidden dirs except `.claude/`), but it's always rooted at the current working directory.

This means:
- You need to `cd` into each project and run `nono trust sign --all`
- If a `CLAUDE.md` changes, the bundle becomes stale (digest mismatch) and must be re-signed
- For projects where `CLAUDE.md` changes frequently, this is ongoing maintenance

### Keyless signing (CI/CD)

For GitHub Actions or other CI environments with OIDC support:

```bash
nono trust sign --keyless CLAUDE.md
```

This uses Sigstore's Fulcio + Rekor for keyless signing via ambient OIDC credentials. Requires `permissions: id-token: write` in GitHub Actions.

A keyless publisher in the trust policy looks like:

```json
{
  "name": "ci-publisher",
  "issuer": "https://token.actions.githubusercontent.com",
  "repository": "org/repo",
  "workflow": ".github/workflows/sign.yml",
  "ref_pattern": "refs/tags/v*"
}
```

Publisher matching supports wildcards (`*`) for repository, workflow, and ref_pattern fields.

---

## Trust Policy Locations & Merging

| Level | Path | Priority |
|-------|------|----------|
| User | `~/.config/nono/trust-policy.json` | Highest |
| Project | `./trust-policy.json` | Lower |
| Default | Built-in `TrustPolicy::default()` | Fallback |

When both user and project policies exist, they merge:
- **Publishers**: Union (deduplicated by name, first wins)
- **Blocklist**: Union of digests and publishers
- **Patterns**: Union of instruction patterns
- **Enforcement**: Strictest wins

When only a project-level policy exists (no user-level), nono prints a warning:

```
Warning: project-level trust-policy.json found but no user-level policy exists.
Project policies are not authoritative without a user-level policy to anchor trust.
```

This is by design — a project shouldn't be able to declare itself trusted without the user establishing the trust root.

---

## Bundle Format

Bundle files follow Sigstore bundle v0.3 JSON format:

```
CLAUDE.md          <- the instruction file
CLAUDE.md.bundle   <- the sidecar bundle
```

A bundle contains:
- **Verification material**: Fulcio certificate (keyless) or public key hint (keyed)
- **DSSE envelope**: in-toto statement with subject name, SHA-256 digest, and cryptographic signature
- **Predicate type**: `nono.dev/attestation/instruction-file/v1` for instruction files, `nono.dev/attestation/trust-policy/v1` for trust policies

### Verification pipeline (per file)

1. Compute SHA-256 digest of file content
2. Load `.bundle` sidecar (if missing: `Unsigned`)
3. Validate predicate type matches expected attestation type
4. Verify subject name in bundle matches the filename
5. Verify bundle digest matches computed file digest
6. Cryptographic verification (ECDSA for keyed, full Sigstore chain for keyless)
7. Extract signer identity and match against trust policy publishers
8. Check blocklist for digest or publisher matches

---

## Runtime Trust Interception (Supervised Mode)

Beyond the pre-exec scan, supervised mode (`--supervised`) includes a `TrustInterceptor` that verifies instruction files at runtime:

- Caches verification results keyed by `(path, inode, mtime_nanos, size)`
- Detects files created during execution (not just pre-existing ones)
- Catches TOCTOU gaps between verification and actual file reads

---

## macOS-Specific: Seatbelt Deny Rules

On macOS, nono injects Seatbelt profile rules for instruction file protection:

- **Deny rules**: Block reading ANY file matching instruction patterns via regex
- **Allow overrides**: Literal path allows for files that passed verification
- Generated for both original and canonical paths (macOS symlink handling)

This means even if the agent process tries to read an unverified `CLAUDE.md` directly, the OS kernel blocks it.

On Linux (Landlock), this OS-level instruction file deny is not available — Landlock is strictly allow-list and cannot express "deny this specific file within an allowed directory."

---

## Verification Outcomes

| Outcome | Meaning | Blocked in Deny? |
|---------|---------|-------------------|
| `Verified` | Bundle valid, signer matches a publisher | No |
| `Unsigned` | No `.bundle` file found | Yes |
| `InvalidSignature` | Crypto verification failed | Yes |
| `UntrustedPublisher` | Valid bundle, signer not in policy | Yes |
| `DigestMismatch` | File content changed since signing | Yes |
| `Blocked` | Digest or publisher on blocklist | **Always** (any mode) |

---

## Practical Decision Tree

```
Do you control the instruction files?
  |
  +-- Yes, I author my own CLAUDE.md
  |     |
  |     +-- Is this a personal dev environment?
  |     |     -> Use --trust-override. Signing your own files
  |     |        against your own key adds friction without security benefit.
  |     |
  |     +-- Is this a team/shared project?
  |           -> Set up keygen + sign workflow. Each developer needs the
  |              signing key or use keyless signing in CI.
  |
  +-- No, I'm consuming third-party instruction files
        |
        -> Full trust verification is the right model. Require signed bundles
           from the project's CI pipeline. Configure publishers in your
           user-level trust policy.
```

## Gotchas

| Issue | Why It Happens |
|-------|---------------|
| `FAIL CLAUDE.md (no .bundle file)` on first run | Default policy has `enforcement=deny` and no publishers. Use `--trust-override` or set up signing. |
| Can't create `trust-policy.json` without signing it | The pre-exec scan requires policy signatures. You can't bootstrap with an unsigned policy unless you use `--trust-override`. |
| Bundle becomes stale after editing `CLAUDE.md` | Digest mismatch. Re-sign with `nono trust sign CLAUDE.md`. |
| `nono trust sign --all` only scans CWD | You must `cd` into each project. There's no recursive multi-project signing. |
| Project policy can't lower enforcement | `enforcement: "audit"` in project policy is overridden by user-level `deny`. Strictest always wins. |
| `trust-policy.json.bundle` needed for `nono run` but not for `nono trust verify` | Asymmetric enforcement. The `trust verify/list` commands warn on unsigned policies but still load them. |
