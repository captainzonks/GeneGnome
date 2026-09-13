// MathJax configuration shared by every page that loads vendor/mathjax-tex-mml-chtml.js.
// Load this script BEFORE the MathJax bundle itself.
//
// Disables the Accessibility/Explorer feature: activating it lazy-loads
// speech-rule-engine and its language data from cdn.jsdelivr.net, which is
// the one part of the MathJax bundle that can't be vendored locally (see
// vendor/README.md). This does not affect screen-reader support — MathJax's
// separate assistive-mml extension (enableAssistiveMml, on by default and
// untouched here) emits hidden MathML for screen readers independently of
// Explorer/enrichment, and is compiled into the combined bundle rather than
// dynamically loaded.
//
// Kept in one file, rather than inlined per page, so a future page that adds
// MathJax doesn't silently reopen this CDN egress by forgetting the config.
window.MathJax = {
    options: {
        enableExplorer: false,
        enableEnrichment: false
    }
};
