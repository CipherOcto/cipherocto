# Branch Protection Rules

## Overview

CipherOcto uses a **Trunk-Based + Feature Streams** branching strategy. All branch protection rules must be configured in GitHub repository settings to enforce this workflow.

> **Location:** Settings → Branches → Add rule

---

## Main Branch Protection

**Branch:** `main`

| Rule | Setting | Rationale |
|------|---------|-----------|
| Require a pull request | ✓ ON | No direct pushes allowed |
| Require approvals | **1** approval | At least one reviewer required |
| Dismiss stale reviews | ✓ ON | Ensure fresh approval after changes |
| Require review from CODEOWNERS | ✓ ON | Domain expert approval |
| Require status checks | ✓ ALL must pass | CI, lint, security must pass |
| Require branches to be up to date | ✓ ON | Prevent merge conflicts |
| Block force pushes | ✓ ON | Prevent history rewrites |
| Allow deletions | ✗ OFF | Protect main branch |

**Required Status Checks (Main) — Enter these exact names:**
```
build-test      (from CI workflow)
test            (from Rust CI workflow)
lint            (from Lint workflow)
security        (from Security Scan workflow)
```

> 💡 **How to add in GitHub UI:**
> 1. Under "Require status checks to pass before merging", click "Add check"
> 2. Start typing each name above — GitHub will auto-suggest matching checks
> 3. Select each one and add it
> 4. Enable "Require branches to be up to date before merging"

---

## Next Branch Protection

**Branch:** `next`

| Rule | Setting | Rationale |
|------|---------|-----------|
| Require a pull request | ✗ OFF | Direct push allowed for integration |
| Require status checks | ✓ ALL must pass | Quality gate before merging to main |
| Block force pushes | ✓ ON | Prevent history rewrites |
| Allow deletions | ✗ OFF | Protect integration branch |

**Required Status Checks (Next) — Enter these exact names:**
```
build-test      (from CI workflow)
test            (from Rust CI workflow)
lint            (from Lint workflow)
security        (from Security Scan workflow)
```

> 💡 **How to add in GitHub UI:**
> 1. Under "Require status checks to pass before merging", click "Add check"
> 2. Start typing each name above — GitHub will auto-suggest matching checks
> 3. Select each one and add it

---

## Feature Branches Pattern Protection

**Pattern:** `feat/**`, `agent/**`, `research/**`, `hotfix/**`

| Rule | Setting | Rationale |
|------|---------|-----------|
| Require a pull request | ✗ OFF | Contributors push directly |
| Require status checks | ✓ ON | Catch issues early |
| Block force pushes | ✓ ON | Clean history |

**Required Status Checks:**
```
build-test      (from CI workflow)
test            (from Rust CI workflow)
lint            (from Lint workflow)
```

---

## Merge Rules

### To Main
- **Via:** Pull Request from `next` or `hotfix/*` only
- **Required:** All status checks passing
- **Required:** At least 1 approval
- **Merge method:** Squash and merge (clean history)

### To Next
- **Via:** Pull Request from `feat/*`, `agent/*`, or direct push
- **Required:** All status checks passing
- **Merge method:** Merge commit (preserve feature history)

---

## Repository Settings Checklist

Go through these in your GitHub repository settings:

### Branch Protection
- [ ] Main branch: Require PR + 1 approval + all checks
- [ ] Main branch: Block force pushes
- [ ] Next branch: Require all checks (no PR required)
- [ ] All branches: Block force pushes on feature patterns

### Rulesets (Recommended)
Create a ruleset for `main` that overrides branch protection:
- [ ] Bypass: No one (admins included)
- [ ] Restrict creations: Only from PRs
- [ ] Required checks: All CI/Lint/Security

---

## CODEOWNERS File

Create `.github/CODEOWNERS` to enforce domain expert reviews:

```
# Core runtime
/crates/runtime/ @core-team

# Agent system
/crates/agents/ @agent-team

# Blockchain/crypto
/crates/blockchain/ @crypto-team

# CLI tools
/crates/cli/ @cli-team

# Docs
/docs/ @docs-team

# Default
* @maintainers
```

---

## Quick Setup Commands

```bash
# Using GitHub CLI (gh)
gh api repos/:owner/:repo/branches/main/protection \
  --method PUT \
  -f required_status_checks='{"strict":true,"contexts":["CI","Lint","Security Scan","Rust CI"]}' \
  -f enforce_admins=true \
  -f required_pull_request_reviews='{"required_approving_review_count":1}' \
  -f restrictions=null
```

---

## Troubleshooting

### "Cannot push to main"
Expected behavior. All changes to main must go through PR.

### "Status checks failing"
Check the Actions tab. All checks must pass before merge.

### "Approval required"
Find a team member with write access to review your PR.
