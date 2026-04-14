# Contributing to VoxFlow

Thank you for your interest in contributing to VoxFlow! This document provides guidelines and instructions for contributing.

## Getting Started

1. Fork the repository and create your branch from `main`.
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/VoxFlow.git
   cd VoxFlow
   ```
3. Install dependencies:
   ```bash
   npm install
   ```
4. Create a `.env` file from the example template:
   ```bash
   cp .env.example .env
   ```
   Then fill in the required values.

## Development

- Start the development server:
  ```bash
  npm run dev
  ```
- Start the Tauri dev environment:
  ```bash
  npm run tauri:dev
  ```

## Code Style

- Follow the existing code style and conventions.
- Use TypeScript strictly — the project has strict compiler options enabled.
- Ensure all new code is covered by existing linting rules.

## Making Changes

1. Create a descriptive branch name:
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```
2. Make your changes with clear, focused commits.
3. Write meaningful commit messages following [Conventional Commits](https://www.conventionalcommits.org/).

## Submitting Changes

1. Push your branch to your fork.
2. Open a Pull Request against the `main` branch.
3. In your PR description:
   - Describe what you changed and why.
   - Reference any related issues.
   - Include screenshots for UI changes.

## Pull Request Checklist

- [ ] Code follows the project's style guidelines.
- [ ] Self-review completed.
- [ ] Changes are tested locally.
- [ ] No new console errors or warnings.
- [ ] Documentation updated (if applicable).

## Reporting Issues

- Search existing issues before creating a new one.
- Provide clear steps to reproduce.
- Include relevant environment details (OS, Node version, etc.).
- Attach screenshots if it's a UI issue.

## Questions?

Feel free to reach out to the maintainers for any questions or guidance.

Happy coding!
