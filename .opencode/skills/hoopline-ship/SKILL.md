---
name: hoopline-ship
description: Ship one Hoopline change through a PR, independent agent review, GitHub CI, merge to main, and Hostim verification.
metadata:
  opencode/autoinvoke: false
---

# Ship Hoopline

Use this workflow only on an explicit request to ship. A request to open a PR
does not authorize a merge or deploy. Identify the worktree, branch, and change
or PR first. Ask if the target is unclear. Follow all tool approval requirements.

1. **Open or update the PR.** Keep unrelated changes out. Use the existing PR
   if it targets `main`; otherwise open one from the change branch. Run relevant
   local checks. Run `just test-all` before merging app changes.
2. **Dispatch one independent review agent.** Give it the worktree, exact base
   and head SHAs, and the intended behavior. Ask for a read-only review of the
   diff in context. Focus on correctness, regressions, missing tests, and
   unnecessary complexity. For affected paths, check booking races, SQLite
   migrations, browser state, and both languages. Ask for findings in severity
   order, with file and line, impact, and the condition that triggers each issue.
   Ask the reviewer to distinguish blockers from suggestions and list checks
   it did not run. Do not ask it to edit files, post on GitHub, or deploy.
3. **Resolve and verify.** Fix blocking findings, add regression tests where
   practical, and have the reviewer recheck substantive fixes. Wait for both
   **Verify** and **Container build** on the final PR head. Use
   `hoopline-release-check` for the exact-SHA checks. Stop if review or CI is
   blocked. Do not bypass branch protection or substitute an older green run.
4. **Merge to `main`.** Require the reviewed head at merge time with
   `gh pr merge --match-head-commit`. Read back the actual merged SHA. Wait for
   both checks on that SHA's `main` push run before deploying.
5. **Deploy and verify.** Confirm the verified SHA is still the tip of `main`.
   If it changed, stop rather than rebuild an unverified revision. Use the
   existing app, `hoopline` in Hostim project `hoops-booking`. Do not trigger a
   duplicate rebuild if that revision is already rolling out or live. If needed
   and permitted, run `hostim apps rebuild hoopline -p hoops-booking` once.
   Use `hoopline-release-check` to verify the completed rollout, running SHA,
   and public `/healthz`. A successful rebuild request is not a verified rollout.

Report the PR link, review outcome, CI results, merged SHA, and production
status. If a gate blocks progress, state where you stopped and what remains.
