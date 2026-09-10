import type { ReactNode } from 'react'
import { css, html } from 'react-strict-dom'
import { colors } from './tokens.css.ts'

const styles = css.create({
  base: {
    color: colors.text,
    fontFamily: 'system-ui, -apple-system, sans-serif',
  },
  title: { fontSize: 28, fontWeight: 700, lineHeight: 1.2 },
  body: { fontSize: 16, lineHeight: 1.4 },
  caption: { fontSize: 13, lineHeight: 1.4, color: colors.textMuted },
})

export type TextVariant = 'title' | 'body' | 'caption'

export function Text({
  variant = 'body',
  children,
}: {
  variant?: TextVariant
  children: ReactNode
}) {
  const Tag = variant === 'title' ? html.h1 : html.p
  return <Tag style={[styles.base, styles[variant]]}>{children}</Tag>
}
