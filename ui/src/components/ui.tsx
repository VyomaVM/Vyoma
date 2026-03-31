import { clsx } from 'clsx';
import { ReactNode } from 'react';

interface SidebarItemProps {
  icon: ReactNode;
  label: string;
  active?: boolean;
  onClick?: () => void;
}

export function SidebarItem({ icon, label, active, onClick }: SidebarItemProps) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        'w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200',
        active
          ? 'bg-orange-500/10 text-orange-400 shadow-sm shadow-orange-900/10 border border-orange-500/10'
          : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
      )}
    >
      {icon}
      {label}
    </button>
  );
}

interface ButtonProps {
  children: ReactNode;
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  size?: 'sm' | 'md' | 'lg';
  disabled?: boolean;
  onClick?: () => void;
  className?: string;
}

export function Button({ children, variant = 'primary', size = 'md', disabled, onClick, className }: ButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={clsx(
        'flex items-center justify-center gap-2 rounded-lg font-medium transition-all duration-200',
        {
          'bg-orange-600 hover:bg-orange-700 text-white': variant === 'primary',
          'bg-slate-700 hover:bg-slate-600 text-slate-200': variant === 'secondary',
          'text-slate-400 hover:text-white hover:bg-slate-800': variant === 'ghost',
          'bg-red-600 hover:bg-red-700 text-white': variant === 'danger',
          'px-3 py-1.5 text-sm': size === 'sm',
          'px-4 py-2 text-sm': size === 'md',
          'px-6 py-3 text-base': size === 'lg',
        },
        disabled && 'opacity-50 cursor-not-allowed',
        className
      )}
    >
      {children}
    </button>
  );
}

interface CardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
}

export function Card({ children, className, hover }: CardProps) {
  return (
    <div
      className={clsx(
        'bg-slate-900 rounded-xl border border-slate-800 overflow-hidden',
        hover && 'hover:border-orange-500/30 transition-colors duration-200',
        className
      )}
    >
      {children}
    </div>
  );
}

interface StatusBadgeProps {
  status: 'running' | 'stopped' | 'error' | 'paused';
}

export function StatusBadge({ status }: StatusBadgeProps) {
  const colors = {
    running: 'bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.4)]',
    stopped: 'bg-slate-500',
    error: 'bg-red-500',
    paused: 'bg-yellow-500',
  };

  return (
    <div className={clsx('w-3 h-3 rounded-sm', colors[status])} />
  );
}

interface EmptyStateProps {
  title: string;
  description?: string;
  icon?: ReactNode;
}

export function EmptyState({ title, description, icon }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-slate-500 gap-2">
      {icon && <div className="p-4 bg-slate-900 rounded-full border border-slate-800">{icon}</div>}
      <h3 className="text-lg font-medium text-slate-400">{title}</h3>
      {description && <p className="text-sm">{description}</p>}
    </div>
  );
}

interface LoadingProps {
  text?: string;
}

export function Loading({ text = 'Loading...' }: LoadingProps) {
  return (
    <div className="flex items-center justify-center py-12">
      <div className="text-slate-500 animate-pulse">{text}</div>
    </div>
  );
}
