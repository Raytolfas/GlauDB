/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        // High contrast minimalist palette
        border: "#e5e5e5",
        background: "#ffffff",
        foreground: "#0a0a0a",
        muted: "#737373",
        accent: "#171717",
      }
    },
  },
  plugins: [],
}
