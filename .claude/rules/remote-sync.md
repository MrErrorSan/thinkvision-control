# Remote sync (multi-machine)

I work on this repo from more than one machine. The **remote is the source of
truth**, not this working copy — unpushed work is work I will lose.

- **Start of session:** `git fetch` and `git status` before touching anything.
  Behind the remote? Pull (or rebase) first. Diverged? Stop and tell me before
  writing any code. (With Anvil installed, its session-start hook has already
  fetched and reports the state in `<anvil-remote-sync>`.)
- **After every unit of work:** commit *and* push. `git push -u github <branch>`
  the first time so the branch tracks. "Committed" is not done — "pushed" is
  done.
- **This repo's remote is named `github`, not `origin`** (it points at
  `MrErrorSan/thinkvision-control`). Push with `git push -u github <branch>`;
  `git push origin ...` will fail here.
- **Whenever I ask for a commit**, push it in the same step unless I say
  otherwise.
- **Never end a session with uncommitted or unpushed changes.** If the work is
  half-finished, commit it as a WIP commit and push anyway.
- Before pushing, mention any untracked files that belong in the repo. Don't
  silently leave new work out of the sync.
- If a push fails (auth, network, protected branch, non-fast-forward), say so
  plainly in your reply. A failed push must never be reported as success.
- `.claude/` is gitignored here (worktrees); this one file is force-added.
