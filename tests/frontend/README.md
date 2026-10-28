# Frontend Smoke Tests

Start the existing Vite dev server, then run from the repository root:

```powershell
$env:HIKYOU_TEST_RUNTIME_MODULES = "<bundled node_modules directory containing playwright>"
$env:HIKYOU_TEST_URL = "http://127.0.0.1:1420"
$env:HIKYOU_TEST_BROWSER_CHANNEL = "msedge"
node scripts/frontend-smoke.mjs
```

`HIKYOU_TEST_URL` defaults to the URL above. Without `HIKYOU_TEST_RUNTIME_MODULES`,
the runner imports `playwright` normally. No installation or production entry edits
are necessary. `HIKYOU_TEST_BROWSER_CHANNEL` is optional: use `msedge` for an
installed Microsoft Edge, or leave it unset for Playwright's bundled Chromium.
No browser is downloaded or installed by the runner.

Each case gets a fresh non-persistent browser context. The browser-runtime case
uses the unmodified entry with no Tauri environment. For native UI cases,
Playwright intercepts the
served `/src/main.ts`, fetches the original response, and prepends the fixture
import. The fixture uses official Tauri `mockIPC`, `mockWindows`, and event mocks;
it sets `globalThis.isTauri` so window-label routing uses the real production path.
The fixture always uses the main window without a saved account. Settings saves
use browser memory. Authentication, account switching, launch, upload, delete,
shell, and unknown IPC operations fail closed. External HTTP and WebSocket requests
are blocked, and service workers are disabled. Production `main.ts` never imports
a test fixture.

The suite contains four browser cases:

- `browser-runtime`: an ordinary browser renders the desktop-required state without native API calls.
- `main-keyboard`: search/filter, keyboard selection, and profile settings.
- `settings-locale`: settings back navigation and English/Japanese persistence.
- `profile-library`: profile inspector, hover-independent library keyboard
  selection, and world datapacks.

Screenshots, console/page errors, blocked network attempts, and IPC evidence are
saved in ignored `.local/frontend-smoke/`. `summary.json` is overwritten each run;
only screenshots named in that summary belong to the latest run. Exit codes:
`0` passed, `1` failed (including unavailable runtime), `2` dev server unavailable.

Route isolation and restart/disposal error handling can also be checked without
a browser, dev server, or external packages:

```powershell
node --test tests/frontend/isolation.test.mjs
```
