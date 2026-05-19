# Vyoma UI

This is the frontend dashboard for the Vyoma project, built with React, TypeScript, Tailwind CSS, and Vite.

## Architecture

- **`src/features/`**: The core of the application is feature-based. Each domain (e.g., VMs, Images, Networks) has its own directory containing the page components and local logic.
- **`src/components/ui/`**: A shared library of reusable, theme-aware components built on top of Radix UI primitives and styled with Tailwind CSS (Shadcn pattern).
- **`src/hooks/`**: Global React hooks, including data-fetching hooks powered by TanStack Query.
- **`src/stores/`**: Global state management using Zustand (e.g., authentication, UI states).
- **`src/lib/`**: Utilities and configuration files, including the core API client.

## Design System

We use a custom design system based on Tailwind CSS. The design tokens are defined in `src/index.css` via CSS custom properties, providing easy theming capabilities (e.g., dark mode out of the box).
- **Backgrounds:** `--vyoma-bg-primary`, `--vyoma-bg-sidebar`
- **Text:** `--vyoma-text-primary`, `--vyoma-text-muted`
- **Accents:** `--vyoma-accent`

Avoid using arbitrary hex codes in components; always reference these predefined semantic variables to maintain consistency.

## Adding a New Feature

1. Create a new directory in `src/features/[feature-name]/`.
2. Implement your page component (e.g., `[Feature]Page.tsx`).
3. If the feature requires data, create a dedicated custom hook in `src/hooks/queries/` using `useQuery` or `useMutation`.
4. Register the new page route in `src/app/router.tsx`.
5. Update the navigation `tabs` array in `src/app/layout.tsx` to include an icon and link to your new feature.

## Development

```bash
# Start the development server
npm run dev

# Run unit and integration tests
npm test

# Build for production
npm run build
```
