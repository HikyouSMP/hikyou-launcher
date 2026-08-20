import React from "react";

export class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="h-screen flex items-center justify-center bg-app-bg p-6">
        <div className="rounded-xl p-5 max-w-95 bg-danger-bg border border-danger-bdr">
          <p className="text-danger text-[13px] font-semibold mb-1">
            Render Error
          </p>
          <p className="text-[rgba(252,165,165,.55)] text-[11px] break-all">
            {this.state.error.message}
          </p>
          <button
            onClick={() => this.setState({ error: null })}
            className="mt-3 px-3 py-1 rounded-md text-[11px] cursor-pointer text-[#a1a1aa] bg-[rgba(255,255,255,.07)] border-0"
          >
            retry
          </button>
        </div>
      </div>
    );
  }
}
