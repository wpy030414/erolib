import { Component, type ReactNode } from 'react';

interface Props { children: ReactNode; }
interface State { error: Error | null; }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{ padding: 48, textAlign: 'center' }}>
          <h2 className="text-h4" style={{ color: 'var(--md-sys-color-error)' }}>Something went wrong</h2>
          <p className="text-body-1" style={{ color: 'var(--md-sys-color-on-surface-variant)', marginTop: 16 }}>
            {this.state.error.message}
          </p>
          <button className="md3-btn md3-btn--filled" style={{ marginTop: 24 }}
            onClick={() => this.setState({ error: null })}>
            Try Again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}