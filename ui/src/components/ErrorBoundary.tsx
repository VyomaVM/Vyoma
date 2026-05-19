import { Component } from 'react';
import type { ReactNode, ErrorInfo } from 'react';
import { AlertTriangle, RefreshCcw } from 'lucide-react';
import { Button } from './ui';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Uncaught error:', error, errorInfo);
  }

  private handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center min-h-[calc(100vh-4rem)] p-8 text-center bg-background">
          <AlertTriangle size={64} className="text-destructive mb-6" />
          <h2 className="text-2xl font-bold text-foreground mb-2">Something went wrong</h2>
          <p className="text-muted-foreground mb-8 max-w-md">
            {this.state.error?.message || 'An unexpected error occurred in the application.'}
          </p>
          <Button onClick={this.handleReset}>
            <RefreshCcw size={16} className="mr-2" /> Try Again
          </Button>
        </div>
      );
    }

    return this.props.children;
  }
}
