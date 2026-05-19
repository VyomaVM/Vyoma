
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { StatusBadge } from '../status-badge';

describe('StatusBadge', () => {
  it('renders running status correctly', () => {
    render(<StatusBadge status="running" />);
    expect(screen.getByText('running')).toBeInTheDocument();
    expect(screen.getByText('running')).toHaveClass('text-green-500');
  });

  it('renders stopped status correctly', () => {
    render(<StatusBadge status="stopped" />);
    expect(screen.getByText('stopped')).toBeInTheDocument();
    expect(screen.getByText('stopped')).toHaveClass('text-slate-500');
  });

  it('renders attestation_failed status correctly', () => {
    render(<StatusBadge status="attestation_failed" />);
    expect(screen.getByText('attestation failed')).toBeInTheDocument();
    expect(screen.getByText('attestation failed')).toHaveClass('text-destructive');
  });
});
