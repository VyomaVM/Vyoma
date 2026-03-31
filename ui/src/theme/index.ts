export const theme = {
  colors: {
    bg: {
      primary: '#020617',
      secondary: '#0f172a',
      tertiary: '#1e293b',
      elevated: '#1e293b',
    },
    text: {
      primary: '#f8fafc',
      secondary: '#94a3b8',
      muted: '#64748b',
    },
    accent: {
      primary: '#f97316',
      primaryHover: '#ea580c',
      secondary: '#22c55e',
      danger: '#ef4444',
    },
    border: {
      default: '#334155',
      hover: '#475569',
    }
  },
  spacing: {
    xs: '0.25rem',
    sm: '0.5rem',
    md: '1rem',
    lg: '1.5rem',
    xl: '2rem',
    '2xl': '3rem',
  },
  radius: {
    sm: '0.375rem',
    md: '0.5rem',
    lg: '0.75rem',
    xl: '1rem',
  },
  font: {
    sans: 'system-ui, -apple-system, sans-serif',
    mono: 'ui-monospace, monospace',
  },
  transition: {
    fast: '150ms ease',
    normal: '200ms ease',
    slow: '300ms ease',
  }
} as const;

export type Theme = typeof theme;
