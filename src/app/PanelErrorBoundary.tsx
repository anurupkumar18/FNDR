import { Component, type ErrorInfo, type ReactNode } from "react";

export class PanelErrorBoundary extends Component<
    { panelName: string; children: ReactNode },
    { error: Error | null }
> {
    state = { error: null as Error | null };

    static getDerivedStateFromError(error: Error) {
        return { error };
    }

    componentDidCatch(error: Error, info: ErrorInfo) {
        console.error(`Panel "${this.props.panelName}" crashed:`, error, info);
    }

    componentDidUpdate(prevProps: { panelName: string; children: ReactNode }) {
        if (prevProps.children !== this.props.children && this.state.error) {
            this.setState({ error: null });
        }
    }

    render() {
        if (this.state.error) {
            return (
                <div className="panel-error-fallback">
                    <h2>{this.props.panelName} hit a runtime error</h2>
                    <p>{this.state.error.message}</p>
                </div>
            );
        }

        return this.props.children;
    }
}
