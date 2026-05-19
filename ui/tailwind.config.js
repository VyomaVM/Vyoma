/** @type {import('tailwindcss').Config} */
export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            "colors": {
                "surface": "#11131d",
                "surface-dim": "#11131d",
                "surface-bright": "#373844",
                "surface-container-lowest": "#0c0e18",
                "surface-container-low": "#191b25",
                "surface-container": "#1e1f2a",
                "surface-container-high": "#282934",
                "surface-container-highest": "#33343f",
                "on-surface": "#e2e1f0",
                "on-surface-variant": "#bbc9cd",
                "inverse-surface": "#e2e1f0",
                "inverse-on-surface": "#2e303b",
                "outline": "#859397",
                "outline-variant": "#3c494c",
                "surface-tint": "#2fd9f4",
                "primary": "#8aebff",
                "on-primary": "#00363e",
                "primary-container": "#22d3ee",
                "on-primary-container": "#005763",
                "inverse-primary": "#006877",
                "secondary": "#f6be39",
                "on-secondary": "#402d00",
                "secondary-container": "#c59300",
                "on-secondary-container": "#433000",
                "tertiary": "#ffd6a3",
                "on-tertiary": "#462b00",
                "tertiary-container": "#ffb13b",
                "on-tertiary-container": "#6e4600",
                "error": "#ffb4ab",
                "on-error": "#690005",
                "error-container": "#93000a",
                "on-error-container": "#ffdad6",
                "primary-fixed": "#a2eeff",
                "primary-fixed-dim": "#2fd9f4",
                "on-primary-fixed": "#001f25",
                "on-primary-fixed-variant": "#004e5a",
                "secondary-fixed": "#ffdfa0",
                "secondary-fixed-dim": "#f6be39",
                "on-secondary-fixed": "#261a00",
                "on-secondary-fixed-variant": "#5c4300",
                "tertiary-fixed": "#ffddb5",
                "tertiary-fixed-dim": "#ffb957",
                "on-tertiary-fixed": "#2a1800",
                "on-tertiary-fixed-variant": "#643f00",
                "background": "#11131d",
                "on-background": "#e2e1f0",
                "surface-variant": "#33343f",
                "space-black": "#04040a",
                "ghost-white": "#f0f0f2"
            },
            "fontFamily": {
                "headline-xl": [
                "Inter"
                ],
                "headline-xl-mobile": [
                "Inter"
                ],
                "headline-lg": [
                "Inter"
                ],
                "headline-md": [
                "Inter"
                ],
                "body-lg": [
                "Inter"
                ],
                "body-md": [
                "Inter"
                ],
                "body-sm": [
                "Inter"
                ],
                "label-md": [
                "JetBrains Mono"
                ],
                "label-sm": [
                "JetBrains Mono"
                ],
                "code-snippet": [
                "JetBrains Mono"
                ]
            },
            "fontSize": {
                "headline-xl": [
                "48px",
                {
                    "lineHeight": "56px",
                    "letterSpacing": "-0.02em",
                    "fontWeight": "700"
                }
                ],
                "headline-xl-mobile": [
                "32px",
                {
                    "lineHeight": "40px",
                    "letterSpacing": "-0.01em",
                    "fontWeight": "700"
                }
                ],
                "headline-lg": [
                "32px",
                {
                    "lineHeight": "40px",
                    "fontWeight": "600"
                }
                ],
                "headline-md": [
                "24px",
                {
                    "lineHeight": "32px",
                    "fontWeight": "600"
                }
                ],
                "body-lg": [
                "18px",
                {
                    "lineHeight": "28px",
                    "fontWeight": "400"
                }
                ],
                "body-md": [
                "16px",
                {
                    "lineHeight": "24px",
                    "fontWeight": "400"
                }
                ],
                "body-sm": [
                "14px",
                {
                    "lineHeight": "20px",
                    "fontWeight": "400"
                }
                ],
                "label-md": [
                "14px",
                {
                    "lineHeight": "20px",
                    "letterSpacing": "0.02em",
                    "fontWeight": "500"
                }
                ],
                "label-sm": [
                "12px",
                {
                    "lineHeight": "16px",
                    "letterSpacing": "0.05em",
                    "fontWeight": "500"
                }
                ],
                "code-snippet": [
                "14px",
                {
                    "lineHeight": "22px",
                    "fontWeight": "400"
                }
                ]
            },
            "borderRadius": {
                "sm": "0.125rem",
                "DEFAULT": "0.25rem",
                "md": "0.375rem",
                "lg": "0.5rem",
                "xl": "0.75rem",
                "full": "9999px"
            },
            "spacing": {
                "unit": "4px",
                "gutter": "24px",
                "margin-mobile": "16px",
                "margin-desktop": "64px",
                "max-width": "1280px"
            }
        },
    },
    plugins: [],
}
