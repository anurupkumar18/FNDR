# FNDR systems atlas slideshow

Download [fndr-systems-atlas.html](fndr-systems-atlas.html) and open it in a
browser. The self-contained file embeds the original diagrams, fonts, logos
and synthetic app screenshots. It works offline without a server.

Autoplay starts on load, advances every 10 seconds and wraps from slide 38
back to slide 1. Pause/Play controls playback. Opening the slide picker or
hiding the tab holds autoplay; returning starts a fresh 10-second interval.
Manual navigation also resets the interval.

- Arrow keys, Page Up/Down or Space navigate with wraparound.
- Home and End jump to the ends. G opens the slide picker; Escape closes it.
- F toggles fullscreen. Plus/minus zoom; zero returns to fit.
- Zoomed slides can be explored by scrolling.
- The address hash preserves the selected slide on reload.
- Browser Print lays out one landscape sheet per slide.

The 38 slides retain the complete judging poster, the atlas overview and all
36 subsystems in A1 through F6 order. Each subsystem retains its original
notes and expandable source references. This is source snapshot 5c92a8a,
dated November 16, 2026, not a claim about the current app or main branch.
This is a browser presentation, not a PPTX file.

## Rebuild and check

The builder accepts the original Claude Design canvas-project directory,
its packaged assets directory and its fonts directory:

```sh
node tools/build-atlas-slideshow.mjs /path/to/canvas-project /path/to/assets /path/to/fonts
node docs/presentations/check-atlas.mjs
```

The builder expects the original asset filenames, including fndr-logo.svg,
kahlert-mark.svg, home-light.jpg, vault-light.png and native-home-seeded.png.
It does not need the Claude canvas support script or live blob URLs.
Build/check use the repository's existing Playwright dependency and local
Chrome, or the browser channel named in PLAYWRIGHT_CHANNEL.

Archivo and IBM Plex Mono are embedded in the supplied artwork under their
SIL Open Font License. Font copyright and license notices remain in the
embedded font metadata; the supplied Archivo license accompanies this deck.
