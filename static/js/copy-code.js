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
    button.textContent = "Copied";
    window.setTimeout(() => {
      button.textContent = previous;
    }, 1200);
  } catch {
    button.textContent = "Select";
  }
});
