---
name: finish-branch
description: Close out a development branch cleanly once work is verified. Use when a feature is verified complete and the user wants to merge, open a PR, or wrap up; or when cleaning up a worktree.
---

# Finish Branch (the finish gate)

Only enter after `code-review` passes (which itself follows `verify-before-done`).

## Process

1. **Confirm the suite is green.**
   ```bash
   gatekeeper check finish -- <your full test command>
   ```
   This runs the command and gates on a zero exit code.
2. **Run the formatters/linters.** For this repo: `cargo fmt --check` and `cargo clippy -- -D warnings`.
3. **Present options** to the user — don't merge unilaterally:
   - merge to the base branch
   - open a PR (summarize the design, the plan, and the verification evidence)
   - keep the branch
   - discard
4. **On merge/PR**, first commit the review artifact for this `HEAD` (`git add docs/reviews/ && git commit -m "docs(review): <feature> code review"`) so the merge records the review, then write the summary from the design, verify, and review docs so the history is legible.
5. **Clean up** the worktree/branch if one was created for this work.

## Don't

- Don't merge before the finish gate passes.
- Don't write the PR summary from memory — quote the design and verification docs.
