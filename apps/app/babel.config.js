const reactStrictPreset = require('react-strict-dom/babel-preset')

// Populated by Expo's Metro babel transformer.
const getPlatform = (caller) => caller?.platform
const getIsDev = (caller) =>
  caller?.isDev ??
  (process.env.BABEL_ENV === 'development' || process.env.NODE_ENV === 'development')

module.exports = (api) => {
  const platform = api.caller(getPlatform)
  const dev = api.caller(getIsDev)
  return {
    presets: [
      // `unstable_transformImportMeta`: wasm-bindgen's loader uses `import.meta.url`.
      ['babel-preset-expo', { unstable_transformImportMeta: true }],
      [reactStrictPreset, { debug: dev, dev, platform }],
    ],
  }
}
