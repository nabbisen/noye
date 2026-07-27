# Introduction

Noye is a lightweight server health-monitoring system that runs entirely
on Cloudflare Workers. It pings configured endpoints (HTTP / HTTPS / TCP
/ SMTP / TLS certificates), records state transitions, and dispatches
notifications (Webhook / Slack / Email) when something changes.

## Who this documentation is for

This book is organized around three personas. Pick the one that matches
your situation; each section assumes only what came before it in that
persona's track.

| If you are… | Start with |
|---|---|
| Trying Noye for the first time | [Why Noye?](./why.md) → [Local development](./dev-idp.md) |
| Running a Noye deployment | [API reference](./api.md) and the [Operations](./deployment.md) section |
| Maintaining or extending Noye | [Architecture](./architecture.md) and [Code organization](./development.md) |

## What Noye is not

Noye is **not** a metrics platform. It does not collect CPU / memory /
disk / latency time-series; it does not aggregate application logs.
Those are well-served by other tools. Noye answers a single question:
"is each of these endpoints currently reachable, and notify me when
that changes."

The boundary is intentional. See [Design notes](./notes.md) for the
trade-offs that shape it.
