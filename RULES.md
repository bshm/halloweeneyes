# Rules

## Communication Rules

- Limit your output to 3 sentences maximum unless explicitly requested
- Enforce strict neutral language - no emotional expressions or subjective words
- DO NOT USE ANY OF THE FOLLOWING WORDS, THEY ARE BLACKLISTED: successfully,
  comprehensive, all, now, latest, perfect, ideal, excellent, great, amazing,
  fantastic, wonderful, impressive, incredible, outstanding, superb, flawless,
  best, worst, easy, simple, hard, difficult, challenging, complicated, messy,
  ugly, beautiful, elegant, efficient, inefficient, effective, ineffective,
  reliable, unreliable, robust, fragile, secure, insecure, fix.
- Always use present tense
- Do not speculate. Only state facts. If unsure say state this.
- Prohibit emoji usage in AI responses
- Never apologize - apologies are emotional expressions
- Prevent creation of final summaries after changes
- Prevents creation of final tests after changes
- Do not use uppercase words in program outputs
- Do not use exclamation marks in program outputs
- Do not create unnecessary documentation without explicit request
- Do not create summaries of changes

## Development Rules

- Follow TDD (Test Driven Development) strictly
- goal is 100% line coverage, 100% branch coverage, 100%  function coverage, 100% region coverage
- use property based testing
- DESIGN.md is read-only and may not be modified under any circumstances

## External Command Execution Rules

- ALWAYS capture both stdout and stderr of external commands to a temporary file and record the exit code
- Use the temporary file as input if you need to filter or parse command output
- Always run command with a timeout to prevent hanging processes

## Decision Making Rules

- Propose tool choices and installations - never decide unilaterally
- Request explicit permission before installing or configuring new tools
- Present options with rationale when multiple approaches exist

## Dependency Management Rules

- Verify latest stable version before adding new dependencies
- Use `cargo search <crate>` to check current version on crates.io
- Document version selection rationale in commit message if not using latest
- Prefer semantic version specifications (e.g., "1.5" over "1.5.0") for flexibility

## Git Rules

- Use .gitignore to exclude build artifacts and temporary files (coverage reports, target/ directory)
- Use .gitignore to exclude IDE configuration (.vscode/ directory)

## Git Commit Rules

- NEVER execute git commands (add, commit, push, etc.) without EXPLICIT user instruction
- When user says "implement X" or "add Y" or "replace Z" - make the changes but STOP before modifying git operations.
  ALWAYS implicitly follow the Step Execution Protocol.
- After completing changes, ALWAYS report status and ask: "Changes complete. Ready to commit?"
- User must explicitly say "commit" to proceed with git operations
