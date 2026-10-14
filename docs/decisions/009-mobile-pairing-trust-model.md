# 009: Mobile pairing trust model for Companion API

FNDR mobile pairing must stay local-first, revocable, and simple enough to
operate without cloud identity.

## Decision

Use a two-step local pairing flow:

1. Mac generates a short-lived 6-digit pairing code and QR payload with
   endpoint metadata (`host`, `port`, `tls`, `cert_fingerprint_sha256`).
2. iPhone submits `POST /v1/pair/complete` with that code and device metadata.

On success, Mac issues an opaque random access token, persists a `MobileDevice`
entry in `StateStore` (`companion_devices`), and requires bearer auth on all
protected routes.

## Trust assumptions

- User is physically present at the Mac when initiating pairing.
- Local network path is potentially hostile, so TLS and pinning are mandatory.
- Token possession grants companion permissions until revoked.

## Security properties

- Pairing codes are single-use and expire (`PAIRING_TTL_MS`).
- Tokens are revocable at device granularity from Mac Settings.
- Revoked or unknown tokens return `403 forbidden`.
- Mobile apps pin to `cert_fingerprint_sha256` from QR payload.
- Pairing is local-network only for MVP; no relay or remote discovery.

## Out of scope (deferred)

- Per-request signatures with device key material.
- Token scope partitioning beyond current permission list.
- Cloud identity or account-linked device enrollment.
- Remote relay/NAT traversal.

## Consequences

- UX stays fast and offline-friendly for home/office usage.
- Revocation semantics are explicit and testable.
- Rotating the Mac TLS cert invalidates previous pin state and requires re-pair.

## Addendum, 2026-10-13: the code is issued only on the Mac, and the server waits for a paired device

**What was wrong.** Step 1 above, "Mac generates a short-lived pairing code", was also mounted as an unauthenticated network route, `POST /v1/pair/start`, which returned the code in its response. With `POST /v1/pair/complete` beside it, any caller that could reach the port got a token with every permission. The server started with every launch of FNDR, bound to `0.0.0.0`, although the companion is parked until after Final and no Mac screen for it is mounted. Checked against a debug build on a seeded synthetic profile over loopback: three unauthenticated requests returned a token, and the token returned five memory cards from `/v1/memories/search`. This breaks both trust assumptions above: nobody was at the Mac, and the hostile network was the one asking.

**Decision.**

1. A pairing code is issued only through the Mac's `companion_start_pairing` command. No network route issues one.
2. Five wrong codes void every code issued so far; the Mac issues a new one. A six-digit code otherwise allows a million guesses within its five minutes.
3. FNDR starts the companion server at launch only when a paired, unrevoked device exists. Otherwise nothing listens on the local network until the Mac UI starts it (`companion_start_server`).

**Options weighed.** Keeping the route behind a loopback check still lets any local process or web page that can reach the port pair (CORS allows every origin). Requiring a click on the Mac to approve each pairing is the better end state and needs the Mac screen the parked companion does not have yet. Binding to loopback until pairing would keep a phone from ever reaching it. Turning the companion off entirely loses nothing today, but the gate above keeps a paired phone working after a restart.

**Checked by** `companion::launch_and_pairing_routes` (the route is gone, the launch gate) and `pairing::tests::five_wrong_codes_void_every_code_the_mac_has_issued`, and by the same three requests against a rebuilt debug app.
