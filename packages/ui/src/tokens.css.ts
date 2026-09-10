import { css } from 'react-strict-dom'

/** Design tokens as StyleX variables; themeable per platform later. */
export const colors = css.defineVars({
  background: '#0b0b0d',
  surface: '#16161a',
  text: '#f4f4f5',
  textMuted: '#a1a1aa',
  accent: '#7c9cff',
  accentText: '#0b0b0d',
  danger: '#ff6b6b',
})

export const spacing = css.defineVars({
  xs: '4px',
  sm: '8px',
  md: '16px',
  lg: '24px',
  xl: '32px',
})

export const radii = css.defineVars({
  sm: '6px',
  md: '12px',
  pill: '999px',
})
