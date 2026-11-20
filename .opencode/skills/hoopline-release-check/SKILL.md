---
name: hoopline-release-check
description: Check Hoopline's GitHub CI, Hostim revision, and public health endpoint without changing the release.
metadata:
  opencode/autoinvoke: false
---

# Hoopline release check

Read only. Do not merge, rerun CI, deploy, restart, or change configuration or credentials.
Do not make bookings or send test alerts.

1. **Identify the target.** Use the requested PR or full commit SHA. For a PR, run
   `gh pr view <pr> -R SimonCW/hoopline --json url,state,baseRefName,headRefOid,mergeCommit,statusCheckRollup`.
   If no target was given, use the current branch's PR if there is one. Ask if the
   target is unclear. Do not assume the local checkout is the release target.
2. **Check GitHub CI.** For an open PR, check **Release gate** in the rollup for
   its current head. If it did not pass, check **Verify** and **Container build**
   for details. Report pre-merge status only. For a PR merged to `main`, use
   `mergeCommit.oid`, not the PR head. For that SHA or an explicit release SHA,
   find its `main` push run and check **Release gate**:

   ```sh
   gh run list -R SimonCW/hoopline --workflow verify.yml --branch main --event push \
     --commit <sha> --json databaseId,headSha,status,conclusion,url
   gh run view <run-id> -R SimonCW/hoopline --json headSha,jobs,url \
     --jq '{headSha,url,jobs:[.jobs[]|{name,status,conclusion}]}'
   ```

   Count only a completed, successful gate for the target. Report pending, failed,
   skipped, or missing checks as such. Do not substitute a passing PR run for a
   missing or failed `main` run.
3. **Check production for a release SHA.** Read the Hostim build and runtime
   status, then check the public endpoint:

   ```sh
   hostim status hoopline -p hoops-booking -o json
   curl --silent --show-error --max-time 10 --output /dev/null --write-out '%{http_code}\n' \
     https://hoopline.simoncw.com/healthz
   ```

   Require `buildStatus: succeeded`, `runtimeStatus: running`, `gitCommit`
   equal to the full target SHA, and HTTP `200`. If a build is in progress or the
   deploy timestamp predates the target build, do not claim that the target is
   live. A `200` alone can come from the previous release.

Report the target SHA, the gate result, and the Hostim revision and HTTP result if
checked. Link the PR or CI run. Report unavailable evidence as unverified.
