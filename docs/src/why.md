# Why Noye?

## The problem

You have a handful of endpoints — a few public services, an internal
admin panel, an SMTP relay, a TLS-protected database — and you want to
know within a minute or two when one of them stops responding. Beyond
that, you want a notification that respects scheduled maintenance,
records every alert for later review, and doesn't lock you into a
single vendor's UI.

Most monitoring stacks treat this as the easy starter case for
something much larger: dashboards, time-series, agent rollouts,
log aggregation. Noye keeps the easy starter case as the entire
product.

## Where Noye fits

Noye is a good fit when you can describe your needs as some
combination of:

- "I have under a few hundred endpoints to watch."
- "I want HTTP / TCP / SMTP / TLS-cert checks; nothing more exotic."
- "I want notifications on state change, not every minute."
- "I want a tamper-evident audit log of who changed what."
- "I want to run on infrastructure I already pay for (Cloudflare)."
- "I'm comfortable trading customisation for less surface area."

## Where Noye is not a fit

You should look elsewhere if any of these matter to you:

- You need **metrics-style** data (latency percentiles, throughput,
  resource graphs).
- You need **agents** running alongside your services.
- You operate **thousands** of targets — Noye's single-Cron-fiber
  monitoring loop is sized for low hundreds; bigger fleets need a
  fan-out architecture that Noye intentionally hasn't built.
- You need **interactive incident workflows** (acknowledgement
  routing, on-call rotation handoff, retrospective tooling).
- You need a **SaaS UI** with a vendor on the other side. Noye is
  software you run, not a service you subscribe to.

## Design principles

Two principles override everything else:

1. **Unix philosophy: minimum features for safety and transparency.**
   Useful-but-out-of-scope is a real category. Features cleared the
   "is this necessary?" bar before they were built; everything else
   sits in the [roadmap](./roadmap-link.md) with the rationale for the
   delay attached.
2. **Accessible by Default and by Design (ABDD).** Every UI page
   is server-rendered semantic HTML, navigable by keyboard, readable
   without CSS or JavaScript, and contrast-checked at compile time
   against WCAG AA. ABDD is the baseline, not the polish step. See
   [Accessibility](./accessibility.md).

If those principles match your taste, the rest of this book will too.
