# Spectator website

Responsive, read-only tournament coverage for phones and desktop browsers.

```powershell
npm install
npm run dev
```

The production build uses the deployed Worker endpoint. Set
`VITE_SNAPSHOT_URL` to use another `/snapshot` endpoint during development.

## Deploy

The site is deployed as Cloudflare Worker static assets at
<https://beeriokart.win>. The custom domain, DNS record, and TLS certificate are
managed by Cloudflare from `wrangler.jsonc`.

```powershell
npm install
npm run deploy
```