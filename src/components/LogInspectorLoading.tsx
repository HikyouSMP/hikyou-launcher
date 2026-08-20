export function LogInspectorLoading() {
  return (
    <div className="tool-window log-inspector-window log-loading-shell">
      <aside className="log-loading-sidebar">
        <div className="log-loading-label" />
        <div className="log-loading-select" />
        <div className="log-loading-label short" />
        <div className="log-loading-select" />
        <div className="log-loading-card">
          <span />
          <span />
          <span />
        </div>
      </aside>
      <main className="log-loading-main">
        <div className="log-loading-head">
          <div className="log-loading-search" />
          <div className="log-loading-pill" />
          <div className="log-loading-pill sm" />
        </div>
        <div className="log-loading-lines">
          {Array.from({ length: 13 }).map((_, i) => (
            <div className="log-loading-line" key={i}>
              <span />
              <b style={{ width: `${42 + ((i * 17) % 48)}%` }} />
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}
