// Extracts react-strict-dom (StyleX) styles to static CSS for web builds.
module.exports = {
  plugins: [
    require('react-strict-dom/postcss-plugin')({
      include: ['src/**/*.{js,jsx,mjs,ts,tsx}', '../../packages/ui/src/**/*.{ts,tsx}'],
    }),
    require('autoprefixer'),
  ],
}
