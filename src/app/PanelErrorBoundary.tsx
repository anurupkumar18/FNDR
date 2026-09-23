import { Component, type ErrorInfo, type ReactNode } from "react";

export class PanelErrorBoundary extends Component<
    { panelName: string; children: ReactNode; onClose?: () => void },
    { error: Error | null }
> {
    state = { error: null as Error | null };

    static getDerivedStateFromError(error: Error) {
        return { error };
    }

    componentDidCatch(error: Error, info: ErrorInfo) {
        console.error(`Panel "${this.props.panelName}" crashed:`, error, info);
    }

    render() {
        if (this.state.error) {
            return (
                <div className="panel-error-fallback" role="alert" aria-live="assertive">
                    <h2>{this.props.panelName} couldn't open</h2>
                    <p>
                        Something unexpected interrupted this view. Your stored memories were not changed.
                    </p>
                    <div className="panel-error-actions">
                        <button
                            type="button"
                            className="ui-action-btn"
                            onClick={() => this.setState({ error: null })}
                        >
                            Try again
                        </button>
                        {this.props.onClose && (
                            <button
                                type="button"
                                className="ui-action-btn"
                                onClick={this.props.onClose}
                            >
                                Return home
                            </button>
                        )}
                    </div>
                </div>
            );
        }

        return this.props.children;
    }
}
