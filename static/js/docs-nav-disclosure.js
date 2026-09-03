// Collapses the docs sidebar's ~140 nav links into a closed-by-default
// <details> on narrow viewports, so keyboard/AT users tabbing through a doc
// page reach the article in a handful of stops instead of tabbing through
// every link first. Desktop stays pixel-identical to a plain always-open
// nav: the breakpoint matches the CSS layout switch, the <summary> is taken
// out of the tab order there so it can't be toggled by keyboard, and a
// stray mouse click that does close it is reverted immediately.
const NARROW_VIEWPORT_QUERY = "(max-width: 1080px)";

const disclosure = document.querySelector(".docs-nav-disclosure");
const summary = disclosure?.querySelector("summary");

if (disclosure && summary) {
  const narrowViewport = window.matchMedia(NARROW_VIEWPORT_QUERY);

  const syncDefaultOpenState = () => {
    const narrow = narrowViewport.matches;
    disclosure.open = !narrow;
    summary.tabIndex = narrow ? 0 : -1;
  };

  syncDefaultOpenState();

  narrowViewport.addEventListener("change", () => {
    // Collapsing while focus sits on a link inside the nav would otherwise
    // drop focus to an implementation-defined spot (often <body>); move it
    // to the summary first so the landing spot stays predictable.
    if (narrowViewport.matches && disclosure.contains(document.activeElement)) {
      summary.focus();
    }
    syncDefaultOpenState();
  });

  disclosure.addEventListener("toggle", () => {
    if (!narrowViewport.matches) {
      disclosure.open = true;
    }
  });

  // The in-article "Browse docs" link jumps to the sidebar's id, which
  // doesn't force an ancestor-details open the way jumping to something
  // *inside* a closed <details> does, since the target is the <aside>
  // itself. Open explicitly so the link keeps working on mobile, including
  // when it's clicked again without a full page reload.
  const openForNavigationTarget = () => {
    if (window.location.hash === "#docs-navigation") {
      disclosure.open = true;
    }
  };

  openForNavigationTarget();
  window.addEventListener("hashchange", openForNavigationTarget);

  document.addEventListener("click", (event) => {
    if (event.target.closest(".docs-mobile-nav-link")) {
      disclosure.open = true;
    }
  });
}
