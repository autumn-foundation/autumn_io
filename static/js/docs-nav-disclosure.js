// Collapses the docs sidebar's ~140 nav links into a closed-by-default
// <details> on narrow viewports, so keyboard/AT users tabbing through a doc
// page reach the article in a handful of stops instead of tabbing through
// every link first. Desktop stays pixel-identical to a plain always-open
// nav: the breakpoint matches the CSS layout switch, and any attempt to
// close the disclosure at desktop widths is reverted immediately.
const NARROW_VIEWPORT_QUERY = "(max-width: 1080px)";

const disclosure = document.querySelector(".docs-nav-disclosure");

if (disclosure) {
  const narrowViewport = window.matchMedia(NARROW_VIEWPORT_QUERY);

  const syncDefaultOpenState = () => {
    disclosure.open = !narrowViewport.matches;
  };

  syncDefaultOpenState();
  narrowViewport.addEventListener("change", syncDefaultOpenState);

  disclosure.addEventListener("toggle", () => {
    if (!narrowViewport.matches) {
      disclosure.open = true;
    }
  });

  // The in-article "Browse docs" link jumps to the sidebar's id, which
  // doesn't force an ancestor-details open the way jumping to something
  // *inside* a closed <details> does, since the target is the <aside>
  // itself. Open explicitly so the link keeps working on mobile.
  if (window.location.hash === "#docs-navigation") {
    disclosure.open = true;
  }

  document.addEventListener("click", (event) => {
    if (event.target.closest(".docs-mobile-nav-link")) {
      disclosure.open = true;
    }
  });
}
