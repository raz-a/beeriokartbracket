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