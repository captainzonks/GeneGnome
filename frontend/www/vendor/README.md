<!--
==============================================================================
README.md - Vendored third-party frontend assets
==============================================================================
Description: Provenance record for third-party JS/font files vendored into
             the frontend so pages work without outbound network access
Author: Matt Barham
Created: 2026-09-13
Modified: 2026-09-13
Version: 1.1.0
==============================================================================
Document Type: Reference
Audience: Developer
Status: Active
==============================================================================
-->

# Vendored Assets

These files are committed as-is from their upstream CDN distribution so the
frontend has no third-party network dependencies at runtime. `frontend/www/`
has no build step — everything here is copied verbatim, not bundled.

To upgrade any of these, download the new version from the same upstream URL
pattern, diff against the changelog, and update this table (version, date,
SHA-256).

The SHA-256 column below is for humans; `SHA256SUMS` in this directory (and
`output/chtml/fonts/woff-v2/SHA256SUMS` for the fonts) is the machine-checkable
version — run `sha256sum -c SHA256SUMS` in either directory to verify the
files on disk haven't drifted from what's recorded here. That's the form you
actually want when a CVE lands and you need to know whether your copy is the
patched one or not.

| File | Version | Upstream | License | SHA-256 |
|---|---|---|---|---|
| `jszip.min.js` | 3.10.1 | https://cdn.jsdelivr.net/npm/jszip@3.10.1/dist/jszip.min.js | MIT / GPLv3 ([JSZIP_LICENSE.md](JSZIP_LICENSE.md)) | `acc7e41455a80765b5fd9c7ee1b8078a6d160bbbca455aeae854de65c947d59e` |
| `chart.umd.min.js` | 4.4.0 | https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js | MIT ([CHARTJS_LICENSE.md](CHARTJS_LICENSE.md)) | `0e2326c6868072bec1592760c6729043caeea2960a2b46cee6a2192aac6abff0` |
| `chartjs-plugin-annotation.min.js` | 3.0.1 | https://cdn.jsdelivr.net/npm/chartjs-plugin-annotation@3.0.1/dist/chartjs-plugin-annotation.min.js | MIT ([CHARTJS_PLUGIN_ANNOTATION_LICENSE.md](CHARTJS_PLUGIN_ANNOTATION_LICENSE.md)) | `f010c3c42842c98381f34ffa5613a99abeea2391080f20cfcf1b1678f3c555fa` |
| `mathjax-tex-mml-chtml.js` | 3.2.2 | https://cdn.jsdelivr.net/npm/mathjax@3.2.2/es5/tex-mml-chtml.js | Apache-2.0 ([MATHJAX_LICENSE.md](MATHJAX_LICENSE.md)) | `300480069078b5892d2363a2b65e2dfbbf30fe5c80f83edbfecf4610fd093862` |
| `output/chtml/fonts/woff-v2/*.woff` (23 files) | 3.2.2 (matches the bundle above) | https://cdn.jsdelivr.net/npm/mathjax@3.2.2/es5/output/chtml/fonts/woff-v2/ | Apache-2.0 ([MATHJAX_LICENSE.md](MATHJAX_LICENSE.md)) | see `output/chtml/fonts/woff-v2/SHA256SUMS` |

## Why the fonts are a separate directory

`mathjax-tex-mml-chtml.js` resolves its CHTML font path at runtime relative to
its own script location (`Package.resolvePath("output/chtml/fonts/woff-v2")`,
via `document.currentScript`). That means the fonts **must** live at
`vendor/output/chtml/fonts/woff-v2/` — moving `mathjax-tex-mml-chtml.js`
without moving this directory alongside it breaks math rendering (fonts
404, falls back to unstyled glyphs) without erroring loudly.

## Known remaining egress: MathJax's accessibility explorer

MathJax's "Explorer" accessibility feature (speech/braille output) lazy-loads
`speech-rule-engine`, `sre-mathmaps-*`, and `wicked-good-xpath` from
`cdn.jsdelivr.net` on demand — this logic ships inside the vendored bundle
itself and isn't something a static file copy can remove. Vendoring the full
SRE dependency chain (locale-specific speech rule maps, several MB) was out
of scope here.

Instead, `mathjax_config.js` (first-party, not vendored) sets
`enableExplorer: false` and `enableEnrichment: false` in `window.MathJax`,
which keeps the Explorer menu item from ever activating and triggering that
load. Every page that loads `mathjax-tex-mml-chtml.js` must load
`mathjax_config.js` immediately before it — this is deliberately a shared
file rather than an inline `<script>` per page, so a future page that adds
MathJax doesn't silently reopen this CDN egress by forgetting the config.

This is defense at the config level, not the network level — nothing stops
a page from loading the MathJax bundle without `mathjax_config.js` and
getting the CDN calls back. If that matters more than the current setup, a
CSP `connect-src`/`script-src` restricted to `'self'` would enforce it at
the browser level instead of relying on every page remembering to include
the config script; that's a larger change (auditing every inline
`style="..."` attribute and WASM's `wasm-unsafe-eval` requirement first) and
wasn't done here.

If MathJax's accessibility features are wanted in the future, vendor the SRE
package chain at that point rather than re-enabling this against the CDN.

**Screen-reader support is unaffected.** `enableAssistiveMml` (a separate
option, default `true`, untouched by this config) is what emits the hidden
MathML screen readers actually consume. Confirmed by grepping the vendored
bundle: its handler serializes the existing MathML tree directly
(`toMML(this.root)`) and doesn't depend on `enableEnrichment`, and the
`AssistiveMmlHandler` class is compiled statically into
`mathjax-tex-mml-chtml.js` rather than dynamically loaded — so disabling
Explorer/enrichment doesn't touch it.
