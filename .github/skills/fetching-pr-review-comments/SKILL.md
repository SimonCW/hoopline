---
name: fetching-pr-review-comments
description: Use when you need to fetch PR review comments from GitHub - uses gh CLI to retrieve reviewer feedback, inline code comments, and review summaries from pull requests
---

# Fetching PR Review Comments

## Overview

Fetch pull request review comments from GitHub using the `gh` CLI tool. This bypasses issues with direct GitHub API access (e.g., SAML SSO blocks on MCP tools) by using the already-authenticated `gh` CLI.

## When to use

- User asks to check PR feedback or reviewer comments
- User wants to implement reviewer-requested changes
- You need to understand what a reviewer asked for before making changes
- Direct GitHub API tools are unavailable or blocked by SSO

## Prerequisites

- `gh` CLI installed and authenticated (`gh auth status` to verify)
- Access to the repository (the working directory should be the repo)

## Commands

### 1. Identify the PR

If you only have a PR number:

```bash
# Get PR metadata (title, author, state, body)
gh pr view <PR_NUMBER>
```

If you need to find a PR:

```bash
# List open PRs
gh pr list

# Find PRs by branch name
gh pr list --head <branch-name>
```

### 2. Fetch review comments (top-level reviews)

These are the summary comments reviewers leave when submitting a review (approve, request changes, comment):

```bash
gh api repos/{owner}/{repo}/pulls/{pr_number}/reviews \
  --jq '.[] | {user: .user.login, state: .state, body: .body}'
```

### 3. Fetch inline code comments

These are comments attached to specific lines of code in the diff:

```bash
gh api repos/{owner}/{repo}/pulls/{pr_number}/comments \
  --jq '.[] | {user: .user.login, path: .path, line: .line, body: .body}'
```

### 4. Fetch general issue-level comments

These are comments on the PR conversation thread (not attached to code):

```bash
gh api repos/{owner}/{repo}/issues/{pr_number}/comments \
  --jq '.[] | {user: .user.login, body: .body}'
```

## Determining {owner}/{repo}

Extract from the git remote:

```bash
gh repo view --json nameWithOwner --jq '.nameWithOwner'
```

## Complete workflow example

```bash
# 1. Get repo identifier
REPO=$(gh repo view --json nameWithOwner --jq '.nameWithOwner')
PR=797

# 2. Get review summaries
echo "=== Reviews ==="
gh api "repos/$REPO/pulls/$PR/reviews" \
  --jq '.[] | "[\(.state)] \(.user.login): \(.body)"'

# 3. Get inline code comments
echo "=== Inline Comments ==="
gh api "repos/$REPO/pulls/$PR/comments" \
  --jq '.[] | "\(.path):\(.line) (\(.user.login)): \(.body)"'

# 4. Get conversation comments
echo "=== Conversation ==="
gh api "repos/$REPO/issues/$PR/comments" \
  --jq '.[] | "\(.user.login): \(.body)"'
```

## Tips

- The `--jq` flag filters JSON output using jq syntax — adjust fields as needed
- Review `state` values: `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, `DISMISSED`
- Inline comments include `path` (file) and `line` (line number in the diff) for precise location
- For large PRs with many comments, add `--paginate` to handle pagination
- If `gh api` returns a 403/SSO error, run `gh auth login` to re-authenticate
