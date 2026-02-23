<!--
  Sync Impact Report
  =============================================================================
  Version change: [unversioned template] → 1.0.0

  Modified principles (template → final):
  - [PRINCIPLE_1_NAME]  → I. Security-First (new)
  - [PRINCIPLE_2_NAME]  → II. Code Quality (new)
  - [PRINCIPLE_3_NAME]  → III. Test-Driven Development (new)
  - [PRINCIPLE_4_NAME]  → IV. Ecosystem Compatibility (new)
  - [PRINCIPLE_5_NAME]  → V. Simplicity (new)
  - (extra)             → VI. Monorepo & Crate Governance (new)
  - (extra)             → VII. Agentic Assistance (added; 7 principles total)

  Added sections:
  - Technology Constraints (replaces [SECTION_2_NAME])
  - Development Workflow   (replaces [SECTION_3_NAME])

  Removed sections:
  - None

  Templates requiring updates:
  ✅ .specify/memory/constitution.md           — this file
  ✅ .specify/templates/plan-template.md       — Constitution Check gates updated
  ⚠  .specify/templates/spec-template.md      — FR section guidance for safe defaults
                                                 pending manual review
  ⚠  .specify/templates/tasks-template.md     — unsafe-audit task note pending
                                                 manual addition

  Deferred TODOs:
  - None. All placeholders resolved.
=============================================================================
-->

# netavark-plugin-pond Constitution

## Core Principles

### I. Security-First

All code paths that interact with kernel APIs (netlink, ioctl, netns),
user-space dataplane components (OVS-DPDK, vhost, memif), or any external
process boundary MUST be reviewed for security implications before merging.

- Every configurable option MUST be safe by default. Absent configuration
  MUST NOT produce a less secure posture than explicit configuration.
- Input validation is MANDATORY at every system boundary: CLI arguments,
  Netavark plugin JSON payloads, netlink responses, and file descriptors
  received from external processes.
- Privilege separation MUST be respected: operations requiring elevated
  capabilities MUST be isolated to the smallest possible scope, minimized in
  duration, and explicitly documented with rationale.
- `unsafe` Rust blocks are a security-sensitive surface. Each MUST carry a
  `// SAFETY:` comment explaining the invariants upheld and MUST be covered
  by tests that exercise the unsafe path.
- Denial-of-service vectors in network/kernel interactions (e.g., unbounded
  resource consumption, kernel object leaks on error paths) MUST be
  considered and mitigated during design.

**Rationale**: This plugin creates and tears down kernel objects (network
namespaces, veth pairs, netlink sockets) as part of the container lifecycle.
An error or exploit at this layer can affect the entire host network stack.

### II. Code Quality

All Rust code in this project MUST meet the following standards before merge.

- Code MUST be idiomatic Rust (2021 edition); antipatterns flagged by
  `cargo clippy -- -D warnings` MUST be resolved, not suppressed without
  documented justification.
- Code MUST be formatted with `rustfmt` using the project configuration.
  `cargo fmt --check` MUST pass in CI.
- All public items (functions, types, traits, modules) MUST have rustdoc
  comments that describe purpose, preconditions, and error conditions.
- `unsafe` blocks MUST be minimized to the smallest enclosing scope. Each
  MUST include a `// SAFETY:` comment (see Principle I).
- Dead code, unused imports, and unused dependencies MUST NOT be merged.
- Panic paths (`unwrap`, `expect`, `panic!`) in library code are forbidden
  except where documented as a programming error that can never occur in
  correct usage; prefer `Result` and meaningful error types.

**Rationale**: Kernel and network code must be correct, readable, and
auditable. Quality gates enforce this consistently across contributors.

### III. Test-Driven Development

Testing is not optional. Tests MUST be written before or alongside
implementation, never deferred to a follow-up task.

- Unit tests MUST cover all non-trivial logic and MUST NOT require a live
  kernel, network namespace, or OVS instance. Abstractions or mocks MUST be
  provided so unit tests run in a standard `cargo test` environment.
- Integration tests that exercise actual kernel APIs (netlink, netns creation,
  veth pairs) MUST be gated with a feature flag or `#[ignore]` annotation and
  MUST include clear documentation on the required execution environment.
- Each PR MUST have no failing tests. Tests MUST NOT be disabled or skipped
  to achieve CI passage.
- Test coverage MUST be sufficient to detect regressions in kernel-interaction
  code paths. Coverage reports SHOULD be reviewed as part of PR review.
- Error paths (teardown failures, kernel rejections, malformed responses) MUST
  be exercised by tests, not left as untested branches.

**Rationale**: The plugin operates in a security-sensitive, stateful
environment. Regressions in setup/teardown code can leave orphaned kernel
objects or broken container networks. Tests are the first line of defense.

### IV. Ecosystem Compatibility

The plugin MUST be a well-behaved participant in the Podman/Netavark
ecosystem.

- The plugin MUST conform to the published Netavark plugin interface contract
  (JSON protocol, setup/teardown lifecycle, network info schema).
- The plugin MUST NOT modify global network state beyond the scope authorized
  by its contract with Netavark. It MUST NOT interfere with networks it does
  not own.
- Setup and teardown operations MUST be idempotent. Partial failures MUST
  attempt cleanup and MUST leave no dangling kernel objects (network
  namespaces, veth pairs, netlink sockets, OVS ports).
- External interfaces MUST remain stable across patch versions. Breaking
  changes to the plugin wire protocol or configuration schema MUST be
  communicated via MAJOR version bumps and migration guidance.
- The plugin MUST be operable by a user familiar with Podman without
  knowledge of internal implementation details.

**Rationale**: Netavark-plugin-pond extends—not replaces—the Podman
ecosystem. Users run it alongside other Podman networks; breaking ecosystem
assumptions causes hard-to-diagnose failures.

### V. Simplicity

Complexity is a cost that MUST be justified.

- YAGNI (You Aren't Gonna Need It): features MUST NOT be implemented
  speculatively. Every feature MUST map to a concrete, documented use case.
- Each crate and module MUST have a single, clearly stated responsibility.
  Modules that grow beyond their stated scope MUST be refactored.
- Configuration options MUST be kept minimal. Every option added expands the
  test matrix and increases the attack surface. Prefer sensible, safe defaults
  over configurability.
- Unjustified complexity (abstractions without multiple concrete uses,
  generics without type-level benefit, indirection without decoupling benefit)
  MUST be identified and refactored before merge.
- The minimum correct solution is preferred. Complexity violations MUST be
  recorded in the Complexity Tracking table of the relevant plan.md and
  justified explicitly.

**Rationale**: This is a plugin for a specific use case (user-space datapath
on single-node podman). Complexity that does not serve this purpose adds
maintenance burden and security surface without benefit.

### VI. Monorepo & Crate Governance

When the project spans multiple Rust crates, they MUST be managed as a
unified Cargo workspace.

- Multiple crates MUST be organized as a Cargo workspace at the repository
  root (`[workspace]` in root `Cargo.toml`).
- Each crate MUST have a clearly scoped purpose documented in its
  `Cargo.toml` description. Organizational-only crates without code are
  forbidden.
- Cross-crate dependencies MUST flow in one direction: higher-level crates
  depend on lower-level crates; circular dependencies are forbidden.
- Shared types, error definitions, and utilities MUST live in a dedicated
  library crate rather than being duplicated across crates.
- Workspace-wide lint configuration (`[workspace.lints]`) and `rustfmt`
  settings MUST be enforced uniformly across all crates.
- Each crate MUST be independently versioned following semantic versioning.
  A breaking change in one crate MUST NOT force a MAJOR bump in unrelated
  crates.

**Rationale**: A monorepo workspace enables atomic refactoring and consistent
tooling while preserving crate independence and clear architectural
boundaries.

### VII. Agentic Assistance

All AI-assisted and automated code processes MUST operate with
transparency, proper attribution, and disciplined reasoning modes:

- **Action documentation**: Every agentic action (code generation, file
  modification, dependency addition, configuration change) MUST produce
  a human-readable record of what was done and why. This record MUST
  be preserved in commit messages, PR descriptions, or inline comments
  as appropriate to the action's scope. Both humans and agents MUST
  trace their decision-making process in the feature specs and plans
  within this RFCs repository, which serves as the project's
  architectural decision record (ADR). Significant design choices,
  trade-off analyses, and rejected alternatives MUST be captured in
  the relevant spec or plan so that future contributors can understand
  not just what was decided but why.
- **Authorship attribution**: All code produced or substantially modified
  by an AI agent MUST be attributed via `Co-Authored-By` in the commit
  trailer. Human reviewers remain accountable for all merged code
  regardless of its origin.
- **Open-world design, closed-world implementation**: During design and
  planning phases, agents MUST operate with an open-world assumption —
  actively exploring alternatives, questioning constraints, and
  surfacing options the human may not have considered. During
  implementation phases, agents MUST switch to a closed-world
  assumption — executing strictly within the boundaries of the approved
  plan, spec, and task list without introducing unplanned scope.
- **Escalation under ambiguity**: When an agent encounters ambiguity,
  conflicting requirements, or a decision that falls outside the
  approved plan, it MUST stop and escalate to the human for guidance
  rather than making an autonomous judgment. Silently resolving
  ambiguity is a violation of this principle.
- **Reversibility bias**: Agents MUST prefer reversible actions over
  irreversible ones. Destructive operations (file deletion, branch
  force-push, data migration) MUST NOT be performed without explicit
  human confirmation, even if the task description appears to authorize
  them.

**Rationale**: AI agents amplify developer velocity but also amplify
mistakes. Transparency and attribution maintain accountability.
Tracing decisions through the RFCs repository creates an institutional
memory that outlives any single contributor or session. Separating
open-world exploration from closed-world execution prevents scope drift
while still leveraging the agent's ability to surface non-obvious
solutions. Mandatory escalation under ambiguity ensures that humans
retain decision authority at every critical juncture.

## Technology Constraints

- **Language**: Rust 2021 edition. The Minimum Supported Rust Version (MSRV)
  MUST be declared in `Cargo.toml` (`rust-version` field) and kept current
  with the stable release channel within reason.
- **Nightly features**: Forbidden unless explicitly approved with documented
  rationale and a plan for migration when the feature stabilizes.
- **Dependencies**: New dependencies MUST be reviewed for security advisories,
  maintenance status, and license compatibility (Apache-2.0 compatible).
  `cargo audit` MUST pass in CI. Prefer well-maintained, audited crates;
  prefer fewer dependencies over more.
- **License**: Apache-2.0. All transitive dependencies MUST be compatible
  with Apache-2.0 redistribution.
- **CI requirements**: The following MUST pass on every PR:
  `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`,
  `cargo audit`, and integration test suite (on a capable runner with netns
  privileges).
- **Platform target**: Linux only. Kernel-specific APIs (netlink, netns,
  veth) are intentionally used; portability to other platforms is not a goal.

## Development Workflow

- All changes MUST be submitted via pull request. Direct commits to `main`
  are forbidden except for automated release tooling.
- PRs MUST pass all CI gates (see Technology Constraints) before merge.
- Security-sensitive changes—any modification touching kernel API
  interactions, privilege escalation, `unsafe` blocks, or the Netavark wire
  protocol—MUST receive explicit reviewer sign-off with security review noted.
- Commit messages MUST follow the Conventional Commits specification
  (e.g., `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`).
- `CHANGELOG.md` MUST be updated with each user-visible change prior to
  release.
- Complexity violations (per Principle V) MUST be documented in
  `plan.md` Complexity Tracking before the PR is opened.

## Governance

This constitution supersedes all other project practices and documentation.
In the event of a conflict, the constitution governs.

Amendments MUST be proposed as a pull request targeting
`.specify/memory/constitution.md` and MUST include:

1. Rationale for the change clearly stated in the PR description.
2. Impact assessment: which existing features, templates, or workflows are
   affected.
3. Updated `CONSTITUTION_VERSION` following semantic versioning:
   - **MAJOR**: Backward-incompatible governance change, principle removal,
     or redefinition that invalidates existing compliance.
   - **MINOR**: New principle or section added, or materially expanded
     guidance.
   - **PATCH**: Clarifications, wording improvements, typo fixes with no
     semantic change.
4. Updated `LAST_AMENDED_DATE` set to the merge date.

Constitution compliance MUST be verified during every PR review. The
Constitution Check gate in `plan-template.md` enumerates the active gates
derived from this document.

The `.specify/memory/constitution.md` file is the single authoritative
source. All speckit templates and agent guidance files MUST reference it.

**Version**: 1.0.0 | **Ratified**: 2026-02-23 | **Last Amended**: 2026-02-23
