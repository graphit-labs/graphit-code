# Contributing to Graphit Code

Thank you for your interest in contributing to **Graphit Code**! This guide will help you get started.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [Code Style](#code-style)
- [Running Tests](#running-tests)
- [Pull Request Process](#pull-request-process)
- [Questions & Support](#questions--support)

---

## Prerequisites

Before you begin, make sure you have the following installed:

| Tool | Minimum Version | Purpose |
|------|----------------|---------|
| [Go](https://go.dev/dl/) | 1.23+ | Primary language |
| [Node.js](https://nodejs.org/) | 20+ | Tooling & scripts |
| [golangci-lint](https://golangci-lint.run/welcome/install/) | latest | Linting |
| [GNU Make](https://www.gnu.org/software/make/) | any | Build automation |

## Getting Started

1. **Fork** the repository on GitHub: [`graphit-labs/graphit-code`](https://github.com/graphit-labs/graphit-code)

2. **Clone** your fork:

   ```bash
   git clone https://github.com/<your-username>/graphit-code.git
   cd graphit-code
   ```

3. **Set up** the development environment:

   ```bash
   make setup-lbug
   ```

4. **Verify** everything works:

   ```bash
   make test
   ```

If all tests pass, you're ready to contribute!

## Project Structure

```
graphit-code/
├── cmd/            # CLI entry points and command definitions
├── internal/       # Private application packages (core logic)
├── docs/           # Documentation and guides
├── .github/        # GitHub templates and CI workflows
├── Makefile        # Build, test, and lint automation
├── go.mod          # Go module definition
└── go.sum          # Go dependency checksums
```

- **`cmd/`** — Contains the `graphit` CLI binary entry point and all command implementations.
- **`internal/`** — Houses the core business logic, organized by domain. Packages here are not importable by external projects.
- **`docs/`** — Project documentation, architecture decisions, and user-facing guides.

## Code Style

### Formatting

All Go code must be formatted with `gofmt`. Run it before committing:

```bash
gofmt -w .
```

### Linting

We use [`golangci-lint`](https://golangci-lint.run/) to enforce code quality:

```bash
golangci-lint run
```

The CI pipeline will reject PRs that fail linting.

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/) for clear, parseable history:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

**Examples:**

```
feat(ast): add support for Python 3.12 match statements
fix(memory): prevent duplicate entries during consolidation
docs(contributing): add section on commit message format
```

## Running Tests

### Unit Tests

```bash
make test
```

### Full CI Suite

Run the complete CI pipeline locally (tests + linting + build):

```bash
make ci
```

> **Tip:** Always run `make ci` before submitting a pull request to catch issues early.

## Pull Request Process

1. **Fork** the repository and create a feature branch from `main`:

   ```bash
   git checkout -b feat/my-awesome-feature
   ```

2. **Make your changes** — keep commits focused and atomic.

3. **Add or update tests** to cover your changes.

4. **Run the full CI suite** locally:

   ```bash
   make ci
   ```

5. **Push** your branch and open a Pull Request against `main`:

   ```bash
   git push origin feat/my-awesome-feature
   ```

6. **Fill out the PR template** completely — describe what changed and why.

7. **Respond to review feedback** promptly. We aim for a collaborative, constructive review process.

### PR Checklist

- [ ] Code compiles without errors
- [ ] All tests pass (`make test`)
- [ ] Linting passes (`golangci-lint run`)
- [ ] Full CI passes (`make ci`)
- [ ] New functionality includes tests
- [ ] Documentation updated if applicable
- [ ] Commit messages follow Conventional Commits

## Questions & Support

- **GitHub Issues** — For bugs and feature requests, use the [issue tracker](https://github.com/graphit-labs/graphit-code/issues).
- **GitHub Discussions** — For questions, ideas, and general discussion, visit [Discussions](https://github.com/graphit-labs/graphit-code/discussions).

---

Thank you for helping make Graphit Code better! 🚀
