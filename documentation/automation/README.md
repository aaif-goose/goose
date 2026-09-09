# Documentation Automation

This directory contains automated pipelines for keeping goose documentation synchronized with code changes.

## Overview

Each automation project tracks specific types of changes and updates corresponding documentation:

| Project | Status | Tracks | Updates |
|---------|--------|--------|---------|
| cli-command-tracking | 🔮 Planned | CLI commands & options | CLI documentation |
| provider-tracking | 🔮 Planned | Supported AI providers | Provider documentation |
| extension-tracking | 🔮 Planned | Built-in extensions | Extension documentation |

## Architecture

Each automation project follows a consistent pattern:

```
project-name/
├── README.md             # Project-specific documentation
├── TESTING.md            # How to test this automation
├── config/               # Configuration files
├── scripts/              # Deterministic extraction/diff scripts
└── recipes/              # AI-powered synthesis/update recipes
```

### Design Principles

1. **Modular**: Each project is self-contained
2. **Testable**: Clear inputs/outputs at each stage
3. **Transparent**: Intermediate files can be inspected
4. **Reusable**: Common patterns across projects

### Hybrid Approach

- **Shell scripts**: Deterministic extraction and comparison
- **AI recipes**: Synthesis and documentation updates

## GitHub Actions Integration

Automation projects can be triggered via GitHub Actions workflows in `.github/workflows/`.

See individual project TESTING.md files for workflow usage.

## Adding New Automations

When creating a new automation project:

1. Create a subdirectory: `documentation/automation/your-project/`
2. Follow the standard structure (README, TESTING, config, scripts, recipes)
3. Create corresponding GitHub Actions workflow (if needed)
4. Update this README with the new project

## Questions?

For project-specific questions, see the README in each project directory.
