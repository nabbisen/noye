# RFC 0006: Slack-specific notification payload formatting

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "Slack-specific notification payload formatting" under `## Feature`
**Estimated size**: small-medium
**Implementation target**: post-0.27.x

---

## Summary

Replace the generic-JSON payload that the `slack` channel currently
sends with a Slack-shaped Block Kit payload that renders cleanly in
Slack: a status-coloured attachment, the target name as the headline,
and the cause / opened-at / target-detail link as fields. Other
channel types (`webhook`, `email`) are unchanged.

## Background

`channel_type = "slack"` shares the same `dispatch_webhook()` path as
`channel_type = "webhook"` today, sending a generic JSON object. Slack
accepts arbitrary JSON to incoming-webhook URLs but renders it as
plain `text` if Block Kit fields are absent. The result is functional
but unstyled.

Operators who specifically picked `slack` as the channel type clearly
want Slack-native rendering; the current behaviour reads as a missing
feature.

## Design

### Channel dispatch fork

`noye_core::notify::dispatch::send_to_channel(channel, event)`
currently dispatches by `channel_type`. The branches today:

| `channel_type` | Dispatcher |
|---|---|
| `webhook` | `dispatch::send_webhook(channel.endpoint, generic_payload)` |
| `slack` | `dispatch::send_webhook(channel.endpoint, generic_payload)` |
| `email` | `dispatch::send_email(channel.endpoint, ...)` |

After this RFC:

| `channel_type` | Dispatcher |
|---|---|
| `webhook` | `dispatch::send_webhook(channel.endpoint, generic_payload)` |
| `slack` | `dispatch::send_slack(channel.endpoint, slack_payload(event))` |
| `email` | `dispatch::send_email(channel.endpoint, ...)` |

The HTTP transport (`POST` to the endpoint URL) and error handling
(429 surfaces, retry-after, etc.) are unchanged — only the payload
shape differs.

### Slack payload shape

A pure helper `slack_payload(event: &NotificationEvent) -> serde_json::Value`
producing a Block Kit document:

```json
{
  "blocks": [
    {
      "type": "header",
      "text": {
        "type": "plain_text",
        "text": "🔴 web-01 is DOWN"
      }
    },
    {
      "type": "section",
      "fields": [
        { "type": "mrkdwn", "text": "*Cause:*\n503 Service Unavailable" },
        { "type": "mrkdwn", "text": "*Opened at:*\n2026-05-04 10:23:11 UTC" }
      ]
    },
    {
      "type": "section",
      "text": { "type": "mrkdwn", "text": "<https://noye.example/targets/tgt-abc?tab=overview|Open in Noye>" }
    }
  ],
  "attachments": [
    { "color": "#dc3545" }
  ]
}
```

| State transition | Header emoji | Attachment colour |
|---|---|---|
| up → down | 🔴 | red (`#dc3545` token equivalent) |
| down → up | 🟢 | green (`#28a745` token equivalent) |

The colour values match the design-token equivalents used by status
badges in the Noye UI, kept intentionally similar to keep the Slack
visual language consistent with the dashboard.

### Helper organization

Three pure helpers in `noye_core::notify::slack`:

- `slack_payload(event) -> serde_json::Value` — top-level builder.
- `slack_header_text(event) -> String` — emoji + name + state phrase.
- `slack_target_link(event, base_url) -> String` — the
  `<URL|label>` mrkdwn for the "Open in Noye" link.

`base_url` for the link comes from the existing `PUBLIC_BASE_URL`
Worker environment variable (set by operators; documented in
`docs/src/setup.md`). When unset, the link section is omitted entirely
rather than producing an `<undefined|...>` Slack link.

### Error handling

Slack returns `200 OK` with body `ok` on success, `4xx` with a JSON
error on rejection. The dispatcher logs a `warn!` on non-2xx and
records the failure via the same path as the generic webhook. Retry
logic stays where it is (none today; ROADMAP item).

## Requirements

- The `slack` channel type MUST send a Block Kit payload as documented
  above; the `webhook` channel type MUST keep sending the generic JSON
  payload unchanged.
- The `slack_payload` helper MUST be a pure function: given a
  `NotificationEvent`, the same `serde_json::Value` is produced
  bit-equivalently across calls. A test pins the byte shape for
  representative up→down and down→up events.
- When `PUBLIC_BASE_URL` is unset, the "Open in Noye" link block MUST
  be omitted; the rest of the payload MUST render correctly.
- The HTTP transport, error handling, and retry behaviour for `slack`
  MUST be identical to `webhook` — only the payload differs.
- The Slack payload MUST NOT exceed Slack's 50-block limit per
  message (plenty of headroom — we use 3).

## Test plan

### Host unit tests (target: `noye_core::notify::slack`)

- `slack_payload_for_up_to_down_event_pins_byte_shape` — fixture
  test asserting a specific JSON structure.
- `slack_payload_for_down_to_up_event_pins_byte_shape`.
- `slack_header_text_uses_red_circle_for_down_and_green_for_up`.
- `slack_payload_omits_link_section_when_public_base_url_unset`.
- `slack_payload_includes_link_section_when_public_base_url_set`.
- `slack_payload_link_label_is_open_in_noye_with_target_id_substituted`.

### Integration test

A miniflare-based integration test that registers a `slack` channel
and triggers a state transition; asserts the dispatcher posts the
Block-Kit-shaped JSON to the channel endpoint (captured by an
in-process mock).

## Security considerations

- **Endpoint validation.** The existing endpoint validation
  (`https://hooks.slack.com/...`) is unchanged. A non-Slack URL with
  the `slack` channel type would receive Block Kit JSON and either
  ignore it or 4xx, but no privacy boundary is crossed because the
  payload is the same data the generic JSON already contains.
- **Information disclosure.** The Slack payload carries the same
  fields the generic webhook payload does (target name, cause,
  opened-at). No new disclosure surface.
- **Markdown injection.** The `cause` field is server-controlled
  (originates from the check result error message). It's escaped per
  Slack's mrkdwn rules to keep `*`, `_`, `<`, `>` from triggering
  unintended formatting. A unit test pins the escaping for a few
  attacker-shaped cause strings.

## Out of scope

- Slack interactive components (action buttons, modals).
- Threading replies under a single Slack message per incident.
- Slack-app OAuth flow; we stay with incoming-webhook URLs as today.
- Per-channel custom message templates.

## Migration / rollout notes

- No D1 schema change; channels already in the `slack` `channel_type`
  bucket flip behaviour automatically on deploy.
- Operators who configured a `slack` channel pointing at a non-Slack
  endpoint (a webhook expecting the generic payload) will see a
  format break. The migration note in the runbook recommends:
  1. Audit existing `slack` channels to confirm endpoints are real
     Slack incoming-webhook URLs, AND
  2. Move any non-Slack endpoint to the `webhook` channel type before
     deploying this change.
- A pre-deploy audit query is documented:
  `SELECT id, name, endpoint FROM notification_channels
   WHERE channel_type = 'slack' AND endpoint NOT LIKE
   'https://hooks.slack.com/%';`
