
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { SidebarItem } from '../sidebar-item';
import { BrowserRouter } from 'react-router-dom';

describe('SidebarItem', () => {
  const renderWithRouter = (ui: React.ReactElement) => {
    return render(<BrowserRouter>{ui}</BrowserRouter>);
  };

  it('renders label and icon correctly', () => {
    renderWithRouter(<SidebarItem icon={<span data-testid="test-icon" />} label="Test Item" to="/test" />);
    expect(screen.getByText('Test Item')).toBeInTheDocument();
    expect(screen.getByTestId('test-icon')).toBeInTheDocument();
  });

  it('applies active styling when active is true', () => {
    renderWithRouter(<SidebarItem icon={<span />} label="Active Item" to="/active" active />);
    const link = screen.getByText('Active Item').closest('a');
    expect(link).toHaveClass('bg-primary/10');
    expect(link).toHaveClass('text-primary');
  });

  it('applies inactive styling when active is false', () => {
    renderWithRouter(<SidebarItem icon={<span />} label="Inactive Item" to="/inactive" active={false} />);
    const link = screen.getByText('Inactive Item').closest('a');
    expect(link).not.toHaveClass('bg-primary/10');
    expect(link).toHaveClass('text-muted-foreground');
  });
});
