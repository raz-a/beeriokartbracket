# Cloudflare Worker

Public snapshot API backed by Cloudflare Workers KV.

## Endpoints

- `GET /snapshot` returns the latest public tournament snapshot.
- `PUT /snapshot` stores a snapshot when supplied with
	`Authorization: Bearer <PUBLISH_TOKEN>`.
- `OPTIONS /snapshot` supports browser CORS preflight requests.

Snapshots must use schema version `1`. A revision older than the currently
stored revision is rejected with `409 Conflict`.

## Local development

```powershell
npm install
npm test
npx wrangler dev --var PUBLISH_TOKEN:local-development-token
```

Local KV data is maintained by Wrangler. `PUBLISH_TOKEN` is a local variable;
the deployed value will be configured as a Cloudflare secret.

## Desktop publisher

For a standalone deployment, build the release executable and place a file named
`publishing.json` in the same directory as the executable. Start from the
repository's `publishing.example.json` and replace the placeholder token with
the value stored in the Worker's `PUBLISH_TOKEN` secret.

```powershell
cargo build --workspace --release
New-Item -ItemType Directory release-package
Copy-Item target/release/beeriokartbracket-gui.exe release-package/
Copy-Item publishing.example.json release-package/publishing.json
```

The release package only needs the `.exe` and `publishing.json`; GUI images and
fonts are embedded in the executable. Keep `publishing.json` private because it
contains the credential that can replace the public snapshot.

The deployed Worker endpoint is used when `publish_url` is omitted. Environment
variables remain a fallback when no sidecar file exists, which is convenient for
local development:

```powershell
$env:BEERIOKART_PUBLISH_TOKEN = "the-same-value-stored-by-wrangler"
cargo run
```

Override the endpoint with `BEERIOKART_PUBLISH_URL`. A malformed sidecar is
reported as a publication failure and is not bypassed with environment values.
Publication failures do not block local saves or tournament actions.