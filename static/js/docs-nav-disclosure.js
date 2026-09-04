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

// Every docs link is a plain full-page navigation (no client-side router),
// so the browser tears down and rebuilds the sidebar on every click. Its
// scroll position isn't part of the browser's native scroll restoration
// (that only covers the document viewport), so without this the nav resets
// to the top on each click, losing your place once you're deep in the tree.
// Persist scrollTop across navigations via sessionStorage, scoped per tab.
{
  const SCROLL_STORAGE_KEY = "docs-nav-scroll";
  const sidebar = document.getElementById("docs-navigation");

  if (sidebar) {
    const saveScrollTop = () => {
      try {
        sessionStorage.setItem(SCROLL_STORAGE_KEY, String(sidebar.scrollTop));
      } catch {
        // Ignore write failures; scroll restoration is a nice-to-have.
      }
    };

    try {
      const savedScrollTop = sessionStorage.getItem(SCROLL_STORAGE_KEY);
      if (savedScrollTop !== null) {
        sidebar.scrollTop = Number(savedScrollTop);
      }
    } catch {
      // sessionStorage unavailable (e.g. privacy mode) — fall back to the
      // default top-of-nav position.
    }

    sidebar.addEventListener("scroll", saveScrollTop, { passive: true });

    // A back-navigation can restore this page from the bfcache instead of
    // reloading it, in which case this script doesn't re-run — so a scroll
    // that happened on a *later* page is still the last thing saved. Without
    // this, navigating forward again (without first scrolling here) would
    // apply that later page's offset instead of this page's actual one.
    window.addEventListener("pageshow", (event) => {
      if (event.persisted) {
        saveScrollTop();
      }
    });
  }
}
