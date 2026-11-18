# 040. Public interactive tutorial uses synthetic IPC

Status: Decided 2026-11-18 at Kunj's request.

## Context

The Tutorials page needs a real FNDR interface that visitors can explore, with an automatic tour. The existing browser preview renders the app shell with synthetic responses, but was built for development and was not included in the public site.

## Decision

Build the existing preview as a separate static entry and embed it in the Tutorials page. Its IPC handler supplies synthetic, in-memory responses and refuses unknown commands. The preview document blocks outbound connections and external images; the frame does not grant popup or top-level navigation permissions. It cannot access a native FNDR backend or capture a screen. Browser preferences and some demo interactions can use local storage, so the tutorial does not claim that it stores nothing.

The walkthrough uses the rendered controls, stops when a visitor interacts, and can be restarted. Reduced-motion settings leave the tour stopped until the visitor starts it. Browser tests exercise the packaged site, guided path, interruption, sample search and blocked network request.

## Alternatives

A video cannot be explored and risks exposing real screen content. A separate imitation of the app would drift from the shipped interface. Reusing the development preview keeps the product UI but means the public build must be checked whenever preview fixtures or app controls change.
