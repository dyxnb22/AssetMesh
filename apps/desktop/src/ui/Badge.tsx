import React from 'react';

export type BadgeVariant = 'mesh' | 'muted' | 'attention' | 'danger';

interface BadgeProps {
  children: React.ReactNode;
  variant?: BadgeVariant;
  className?: string;
}

export const Badge: React.FC<BadgeProps> = ({
  children,
  variant = 'muted',
  className = '',
}) => {
  const getStyle = (): React.CSSProperties => {
    switch (variant) {
      case 'mesh':
        return {
          backgroundColor: 'var(--color-mesh-bg)',
          color: 'var(--color-mesh)',
          border: '1px solid var(--color-mesh)',
        };
      case 'attention':
        return {
          backgroundColor: 'var(--color-attention-bg)',
          color: 'var(--color-attention)',
          border: '1px solid var(--color-attention)',
        };
      case 'danger':
        return {
          backgroundColor: 'var(--color-danger-bg)',
          color: 'var(--color-danger)',
          border: '1px solid var(--color-danger)',
        };
      case 'muted':
      default:
        return {
          backgroundColor: 'var(--color-surface-active)',
          color: 'var(--color-muted)',
          border: '1px solid var(--color-border)',
        };
    }
  };

  return (
    <span
      className={`badge ${className}`}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        padding: '1px 6px',
        borderRadius: 'var(--radius-sm)',
        fontSize: '11px',
        fontWeight: 500,
        lineHeight: '16px',
        ...getStyle(),
      }}
    >
      {children}
    </span>
  );
};
