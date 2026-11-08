// Applies the saved theme before the page paints; the header button cycles Auto, Light, Dark.
(() => {
  const root = document.documentElement;
  const order = ["auto", "light", "dark"];
  const names = { auto: "Auto (follows your device)", light: "Light", dark: "Dark" };
  const bg = { light: "#F5F5F5", dark: "#000000" };
  let theme = "auto";
  try { theme = order.includes(localStorage.getItem("theme")) ? localStorage.getItem("theme") : "auto"; } catch {}

  function apply() {
    if (theme === "auto") root.removeAttribute("data-theme"); else root.dataset.theme = theme;
    // The browser bar color and the app picture follow the device setting by themselves; point them at the chosen theme.
    document.querySelectorAll('meta[name="theme-color"]').forEach(m => { m.dataset.auto ??= m.content; m.content = bg[theme] || m.dataset.auto; });
    document.querySelectorAll("picture source").forEach(s => {
      s.dataset.media ??= s.media;
      s.media = theme === "auto" ? s.dataset.media : theme === "dark" ? "all" : "not all";
    });
    const b = document.querySelector(".theme");
    if (b) b.setAttribute("aria-label", "Theme: " + names[theme]), b.title = "Theme: " + names[theme];
  }

  apply();
  document.addEventListener("DOMContentLoaded", apply);
  document.addEventListener("click", e => {
    if (!e.target.closest(".theme")) return;
    theme = order[(order.indexOf(theme) + 1) % order.length];
    try { localStorage.setItem("theme", theme); } catch {}
    apply();
  });
})();
