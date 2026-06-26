/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: "#0f1117",
        card: "#1a1f2e",
        accent: "#6366f1",
        "accent-2": "#22d3ee",
        positive: "#22c55e",
        negative: "#ef4444",
        muted: "#64748b",
      },
    },
  },
  plugins: [],
};
