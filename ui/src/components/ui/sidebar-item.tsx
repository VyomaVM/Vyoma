import { clsx } from 'clsx';
import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';

interface SidebarItemProps {
  icon: ReactNode;
  label: string;
  active?: boolean;
  to?: string;
  onClick?: () => void;
}

export function SidebarItem({ icon, label, active, to, onClick }: SidebarItemProps) {
  const content = (
    <>
      {icon}
      {label}
    </>
  );

  const className = clsx(
    'w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200',
    active
      ? 'bg-primary/10 text-primary shadow-sm border border-primary/10'
      : 'text-muted-foreground hover:bg-muted hover:text-foreground'
  );

  if (to) {
    return (
      <Link to={to} className={className} onClick={onClick}>
        {content}
      </Link>
    );
  }

  return (
    <button onClick={onClick} className={className}>
      {content}
    </button>
  );
}
