# AGENTS.md

Project-specific rules for coding agents. Details: `docs/DEVELOPMENT.md`.
Organize files properly: Keep folders and files logically structured by feature and responsibility.
- Keep folders and files well-organized, modular, and maintainable.
- Use skills for UI/UX and for the desktop-app feel.
- Never put the entire application logic in one file. Separate concerns into appropriate modules and reusable components.
- Build reusable components, hooks, utilities, and shared logic wherever appropriate. Avoid code duplication and follow consistent naming conventions.
- Use the shared `Tooltip` (`src/components/ui/tooltip.tsx`) instead of native `title` for icon-only controls and truncated text.
- Use the `find-skills` skill to discover and apply relevant skills for this project. Check already-installed skills and never install or add duplicates.
- Use semantic design tokens for colors, typography, spacing, borders, and other design properties. Never hardcode design values in components; define and reuse centralized tokens.
- Add relevant theme reference URLs and useful resource links to `AGENTS.md`.
- Follow the existing project structure, conventions, and tech stack. Avoid unnecessary files, dependencies, and architectural changes.
- Prioritize clean architecture, maintainability, performance, accessibility, and consistency.
Also, structure the design system  to support multiple themes in the future. Switching themes should instantly update the entire UI, including backgrounds, colors, borders, and other design tokens. Use centralized semantic CSS variables and theme configurations to make adding or changing themes easy without modifying individual components.
donot use broswer tools to verify user will review manually 

## Problem solving

- When a problem is hard to diagnose or the fix is unclear, research it before
  writing code. Do not guess, and do not settle for an outdated method.
- Use the latest stable, recommended approach. No hacks, temporary fixes or
  workarounds.
- For anything uncertain, check official sources first: Tauri, Rust crate docs,
  React/Tailwind docs. Official documentation beats random tutorials.
- Confirm the version in use before copying a solution from the web.

## React

-Avoid useEffect: Do not use React's useEffect by default. Prefer event handlers, derived state, and proper data flow. Use it only when genuinely necessary.
## Links
 
- Design tokens: `src/index.css`
- shadcn/ui themes: https://ui.shadcn.com/themes
- tweakcn theme editor: https://tweakcn.com
- Tailwind CSS v4 theme variables: https://tailwindcss.com/docs/theme
- Radix Colors: https://www.radix-ui.com/colors
- Tauri v2 docs: https://v2.tauri.app/
- Skill registry: https://skills.sh/
