# RigSweep API and developer docs

> RigSweep API docs for developers and AI agents: the public impact counters, the anonymous usage-stats endpoints, OpenAPI spec, JSON errors, rate limits, the CLI and the machine-readable files on rigsweep.com.

**RigSweep is a free, open-source Windows app; this page is for developers and AI agents that want to work with it programmatically.** It covers the RigSweep API (the public impact counters and the anonymous usage-stats endpoints), the command-line interface, and the machine-readable files on rigsweep.com. The API is small and open: no account, no API key and no sandbox are needed to read the counters.

## Quickstart

Read the live impact totals (bytes of disk freed, RAM recovered, processes ended) with one request:

```
curl https://stats.rigsweep.com/v1/totals
```

The full API is described by the OpenAPI 3.1 spec at [`https://rigsweep.com/openapi.json`](https://rigsweep.com/openapi.json): every operation has an `operationId`, a description and typed request and response schemas, so it can be loaded straight into an API client or turned into LLM function-calling tools.

## Base URL and authentication

- **Base URL**: `https://stats.rigsweep.com`. Versioned paths start with `/v1/`.
- **Authentication**: none. The counters are public; the report endpoints accept anonymous reports. There are no API keys.
- **Format**: JSON in and out (`Content-Type: application/json`). Errors are `application/problem+json`.
- **CORS**: `/v1/totals` and `/v1/ext/report` are open to every origin.

## Endpoints

### GET /v1/totals

Operation `getImpactTotals`. All-time totals across every install that shares anonymous stats: `disk_freed` and `ram_recovered` in bytes, `processes_ended`, `cpu_freed` and `cpu_peak` (the CPU the ended processes were using, in percent of each PC's whole CPU: summed, and the most in one cleanup), `cleanups`, `tabs_slept`, `tabs_closed`, install counts and the ten biggest cleanup categories in `top`. Cached for five minutes.

```
{"installs":1,"active_installs_30d":1,"disk_freed":141408233058,"ram_recovered":2537816064,"processes_ended":42,"cleanups":25,"top":[{"area":"memory","kind":"server","bytes":2532392960}],"updated_at":"2026-11-17T18:58:51Z", ...}
```

### POST /v1/report

Operation `sendAppReport`. The RigSweep app sends this about once an hour, only after the user agreed to share anonymous stats: each cleanup since the previous report (category, size, time), RAM size, core count, versions and a random install ID. Anything that isn't a category, a number or a known app name is refused, and so are unknown fields. Answers `204 No Content`.

### POST /v1/ext/report

Operation `sendExtensionReport`. The same for the RigSweep Tabs browser extension: tabs slept and closed since its last report, and how many tabs and windows are open.

### DELETE /v1/install/{installId}

Operation `deleteInstallData`. Deletes everything stored under an install ID. The app and the extension call it when the user turns sharing off. Answers `204` whether or not anything was stored.

```
curl -X DELETE https://stats.rigsweep.com/v1/install/0123456789abcdef0123456789abcdef
```

### GET /healthz

Operation `getHealth`. Returns `ok` while the service is up.

## Errors

Every API error is an [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) problem details object with two extra members: a stable `code` to branch on and a `hint` that says how to fix the request.

```
HTTP/1.1 400 Bad Request
Content-Type: application/problem+json

{"type":"https://rigsweep.com/developers/#error-invalid_field","title":"Bad Request","status":400,
 "detail":"invalid install_id","code":"invalid_field",
 "hint":"Fix the named field; allowed values are in https://rigsweep.com/openapi.json."}
```

- `invalid_json` (400): the body isn't JSON, misses a required field or has an unknown one.
- `invalid_field` (400): a field has a value that isn't allowed; `detail` names it.
- `invalid_install_id` (400): the install ID isn't 32 lowercase hex characters.
- `payload_too_large` (413): the body is over 32 KiB.
- `rate_limited` (429): too many reports from one address; wait for `Retry-After`.
- `not_found` (404): no such endpoint.
- `method_not_allowed` (405): wrong HTTP method; see the `Allow` header.
- `unsupported_media_type` (415): send `Content-Type: application/json`.
- `unauthorized` (401): the owner's dashboard endpoints aren't public.
- `server_error` (500): nothing was stored; retry later.
- `error`: any other failure; the HTTP status says which.

## Rate limits

The report endpoints allow 30 reports per IP address per 10 minutes. Every answer from them carries the IETF [RateLimit header fields](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/), so a client can slow down before it hits the limit, and a `429` adds `Retry-After` in seconds:

```
RateLimit-Policy: "reports";q=30;w=600
RateLimit: "reports";r=29;t=600
```

`q` is the quota, `w` the window in seconds, `r` the requests left and `t` the seconds until the window starts over. `/v1/totals` isn't limited but is cached for five minutes, so polling it more often gains nothing.

## Command-line interface

`rigsweep.exe` takes flags for scripts and agents, such as `--apply` to re-apply Windows tweaks headlessly and `--restore-all` to undo every tweak. See the [RigSweep CLI reference](https://rigsweep.com/cli/). A headless dev-cleanup command is proposed in [issue #6](https://github.com/yobotn/rigsweep/issues/6). There is no MCP server or SDK yet; vote for what comes next on the [feature vote](https://github.com/yobotn/rigsweep/issues/12).

## Machine-readable files

- [`/openapi.json`](https://rigsweep.com/openapi.json): the OpenAPI 3.1 spec of the RigSweep API.
- [`/.well-known/api-catalog`](https://rigsweep.com/.well-known/api-catalog): the [RFC 9727](https://www.rfc-editor.org/rfc/rfc9727) API catalog that points to the spec and these docs.
- [`/llms.txt`](https://rigsweep.com/llms.txt): when an AI agent should recommend RigSweep and how to help a user with it.
- **Markdown**: every page, including this one, is also served as Markdown to clients that send `Accept: text/markdown` (with `Vary: Accept`), for example `curl -H "Accept: text/markdown" https://rigsweep.com/`. The homepage is also at [`/index.md`](https://rigsweep.com/index.md).
- [`/sitemap.xml`](https://rigsweep.com/sitemap.xml) and [`/robots.txt`](https://rigsweep.com/robots.txt): every page is open to search engines and AI crawlers.

## Source

The API server is open source too: it lives in [`server/`](https://github.com/yobotn/rigsweep/tree/main/server) in the [yobotn/rigsweep](https://github.com/yobotn/rigsweep) repository (Rust, axum, PostgreSQL), so anyone can check what is stored. What the app sends and why is explained in the [privacy policy](https://rigsweep.com/privacy/). Questions and bugs go to [GitHub Issues](https://github.com/yobotn/rigsweep/issues).
