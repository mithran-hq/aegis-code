# Implementation Plan

This repository is tracked by a GitHub issue train.

## Strategy

Aegis Code starts as a Codex-derived harness and adds Aegis control in layers:

1. repo bootstrap and governance
2. upstream source import and sync strategy
3. product rename to `aegis`
4. method state and evidence receipts
5. sensitive tool mediation through Aegis Secret
6. Aegis Engine event emission and context-pack learning
7. optional Aegis Agent Runtime execution substrate
8. provider expansion, including native Anthropic
9. distribution through GitHub Releases, Homebrew, and npm

## Issue Train

The parent issue is the coordination artifact. Child issues are implementation
units and should be small enough to land independently.

The current intended child tasks are:

1. Bootstrap repository, license, governance, and local workflow.
2. Record v1 architecture and product-boundary ADRs.
3. Import upstream Codex source and preserve attribution.
4. Rename binary and package surfaces to `aegis`.
5. Establish upstream sync strategy.
6. Map Codex architecture and extension points.
7. Define Aegis Code method state model.
8. Implement method-state persistence.
9. Implement prompt assembly with Aegis context layers.
10. Implement Aegis Code context-pack schema.
11. Implement context-pack loader and validator.
12. Implement context-pack promotion and rollback commands.
13. Add native Aegis Secret command mediation.
14. Define sensitive command policy contract.
15. Implement tool-call preflight gates.
16. Implement evidence receipt model.
17. Implement evidence collection for tests and builds.
18. Implement GitHub issue-train validator.
19. Implement PR readiness validator.
20. Implement adversarial review command.
21. Integrate Aegis Agent Runtime as optional execution substrate.
22. Define Aegis Code runtime event schema.
23. Implement Aegis Engine event sink.
24. Implement Aegis Engine alert ingestion.
25. Implement learned-pack candidate compiler.
26. Add provider abstraction review.
27. Implement native Anthropic provider support.
28. Preserve OpenAI-compatible provider support.
29. Preserve local OSS provider support.
30. Add provider routing policy.
31. Implement Aegis Code config migration.
32. Implement managed guidance installers.
33. Implement MCP server surface.
34. Implement non-interactive exec mode.
35. Implement TUI status panels for method state.
36. Implement sandbox policy integration.
37. Add local CI and test matrix.
38. Add golden-path integration tests.
39. Add security and privacy redaction tests.
40. Add documentation site content.
41. Add release pipeline.
42. Add Homebrew distribution.
43. Add npm wrapper distribution.
44. Add upgrade and version diagnostics.
45. Supersede the old bruno-gate planning repo.

## Closure

The plan is complete when `aegis` can run the inherited coding loop,
enforce method gates, record evidence, mediate sensitive commands, emit Aegis
events, load promoted context packs, and ship installable release artifacts.
