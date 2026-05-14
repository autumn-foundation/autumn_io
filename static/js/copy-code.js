const READY_LABEL = "Copy";
const READY_ARIA_LABEL = "Copy code to clipboard";
const COPIED_LABEL = "Copied";
const COPIED_ARIA_LABEL = "Copied code to clipboard";
const SELECT_LABEL = "Select";
const SELECT_ARIA_LABEL = "Select code manually";
const RESET_DELAY_MS = 1200;

document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-copy-button]");
  if (!button) {
    return;
  }

  const block = button.closest("[data-copy-code]");
  const code = block?.querySelector("code");
  if (!code) {
    return;
  }

  try {
    await navigator.clipboard.writeText(code.innerText);
    if (button.dataset.copyResetTimer) {
      window.clearTimeout(Number(button.dataset.copyResetTimer));
    }
    button.textContent = COPIED_LABEL;
    button.setAttribute("aria-label", COPIED_ARIA_LABEL);
    button.dataset.copyResetTimer = window.setTimeout(() => {
      button.textContent = READY_LABEL;
      button.setAttribute("aria-label", READY_ARIA_LABEL);
      delete button.dataset.copyResetTimer;
    }, RESET_DELAY_MS);
  } catch {
    if (button.dataset.copyResetTimer) {
      window.clearTimeout(Number(button.dataset.copyResetTimer));
      delete button.dataset.copyResetTimer;
    }
    button.textContent = SELECT_LABEL;
    button.setAttribute("aria-label", SELECT_ARIA_LABEL);
  }
});
