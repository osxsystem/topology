# Topology

The agent-tooling framework's domain glossary. Topology installs *operators* (instincts, skills, gates,
scans) that shape agent behaviour across harnesses, with the `gatekeeper` binary as the enforcer.

## Language

**Session**:
A single temporal agent context window — it ends, after which a fresh session starts with no prior
conversation. Memory exists to bridge between sessions.
_Avoid_: using "session" for the directory of files that survive context windows (those are artifacts).

**Artifact**:
A file produced by a workflow stage that outlives the session — a research note, design spec, plan,
critic report, or a memory handoff/compaction. "Artifact" is the canonical word; the memory protocol
stores its generated artifacts under `memory/artifacts/`.
_Avoid_: output, document, "session file".

**Handoff artifact**:
A memory artifact that captures work state (goal, what's done, next step, key files, decisions) so a
fresh session can resume without re-deriving context.

**Compaction artifact**:
A memory artifact that is a brief, structured summary of long context, written to keep what matters when
a session's context window fills.
