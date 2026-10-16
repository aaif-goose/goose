import { Component, type ReactNode } from 'react';

interface PluginErrorBoundaryProps {
  fallback: ReactNode;
  resetKey: unknown;
  children: ReactNode;
}

export class PluginErrorBoundary extends Component<PluginErrorBoundaryProps, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: Error) {
    console.warn('[client-extensions] Plugin failed while rendering:', error);
  }

  componentDidUpdate(previous: PluginErrorBoundaryProps) {
    if (this.state.failed && previous.resetKey !== this.props.resetKey) {
      this.setState({ failed: false });
    }
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
