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

The GUI publishes automatically after each successful local save when
`BEERIOKART_PUBLISH_TOKEN` is present in its environment. The deployed endpoint
is the default. Override it for local development with
`BEERIOKART_PUBLISH_URL`.

```powershell
$env:BEERIOKART_PUBLISH_TOKEN = "the-same-value-stored-by-wrangler"
cargo run
```

Do not commit the token. Publication failures do not block local saves or
tournament actions; the GUI reports them separately.