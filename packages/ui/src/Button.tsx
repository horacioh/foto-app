import type { ReactNode } from 'react'
import { css, html } from 'react-strict-dom'
import { colors, radii, spacing } from './tokens.css.ts'

const styles = css.create({
  base: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    paddingBlock: spacing.sm,
    paddingInline: spacing.md,
    borderRadius: radii.pill,
    borderWidth: 0,
    fontSize: 16,
    fontWeight: 600,
    cursor: 'pointer',
  },
  primary: {
    backgroundColor: colors.accent,
    color: colors.accentText,
  },
  secondary: {
    backgroundColor: colors.surface,
    color: colors.text,
  },
  disabled: {
    opacity: 0.5,
  },
})

export type ButtonVariant = 'primary' | 'secondary'

export function Button({
  variant = 'primary',
  disabled = false,
  onPress,
  children,
}: {
  variant?: ButtonVariant
  disabled?: boolean
  onPress: () => void
  children: ReactNode
}) {
  return (
    <html.button
      disabled={disabled}
      onClick={onPress}
      style={[styles.base, styles[variant], disabled && styles.disabled]}
    >
      {children}
    </html.button>
  )
}
