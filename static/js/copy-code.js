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
    const previous = button.textContent;
    const previousLabel = button.getAttribute("aria-label");
    button.textContent = "Copied";
    button.setAttribute("aria-label", "Copied code to clipboard");
    window.setTimeout(() => {
      button.textContent = previous;
      if (previousLabel) {
        button.setAttribute("aria-label", previousLabel);
      }
    }, 1200);
  } catch {
    button.textContent = "Select";
    button.setAttribute("aria-label", "Select code manually");
  }
});
