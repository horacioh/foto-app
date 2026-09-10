import type { ReactNode } from 'react'
import { css, html } from 'react-strict-dom'
import { colors, spacing } from './tokens.css.ts'

const styles = css.create({
  root: {
    display: 'flex',
    flexDirection: 'column',
    flexGrow: 1,
    minHeight: '100%',
    backgroundColor: colors.background,
    color: colors.text,
    paddingTop: spacing.xl,
    paddingBottom: spacing.lg,
    paddingInline: spacing.md,
    gap: spacing.md,
  },
})

/** Root container for a screen. Sets strict layout conformance for RN. */
export function Screen({ children }: { children: ReactNode }) {
  return (
    <html.div data-layoutconformance="strict" style={styles.root}>
      {children}
    </html.div>
  )
}
