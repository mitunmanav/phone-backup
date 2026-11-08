// Draws the site's pixel art: the courier from the app's backup scene (src/ui/courier.rs), the icons, the phone and the PC.
// Every picture is whole squares in an SVG; CSS sets its size and which parts show.
(() => {
  // Letters are colors: courier cap C, skin S, eye K, jacket J, trousers P, feet F; I outline, A copper, D dark copper, B screen, G shelf, W cloud.
  const ink = { C: "var(--accent)", S: "var(--skin)", K: "#141414", J: "var(--jacket)", P: "var(--pants)", F: "var(--outline)",
                I: "currentColor", A: "var(--accent)", D: "var(--deep)", B: "var(--surface)", G: "var(--ground)", W: "var(--cloud)" };
  const HEAD = ["..CCCC..", "..CCCCCC", "..SSSS..", "..SSKS..", "..SSSS.."];
  const FRONT = ["..CCCC..", ".CCCCCC.", "..SSSS..", "..KSSK..", "..SSSS.."];
  const BODY = [".JJJJJJ.", "SJJJJJJS", "..JJJJ.."];
  const ARMS_UP = [".JJJJJJ.", ".JJJJJJ.", "..JJJJ.."];
  const ARM_OUT = [".JJJJJJ.", "SJJJJJJ.", "..JJJJ.."];
  const LEGS = [["..P..P..", "..F..F.."], [".P....P.", "FF....FF"], ["...PP...", "...FF..."], ["..P..P..", "..F...F."]];

  const ART = {
    down: [".....II.....", ".....II.....", ".....II.....", ".....II.....", "..IIIIIIII..", "...IIIIII...", "....IIII....", ".....II.....", "............", ".I........I.", ".IIIIIIIIII.", "............"],
    box: ["............", ".IIIIIIIIII.", ".IAAADDAAAI.", ".IAAADDAAAI.", ".IIIIIIIIII.", ".IAAAAAAAAI.", ".IAAAAAAAAI.", ".IAAAAAAAAI.", ".IAAAAAAAAI.", ".IIIIIIIIII.", "............", "............"],
    screen: ["............", "IIIIIIIIIIII", "IBBBBBBBBBBI", "IBBBBBBAABBI", "IBBAABBAABBI", "IBBAABBBBBBI", "IBBBBBBBBBBI", "IIIIIIIIIIII", ".....II.....", "...IIIIII...", "............", "............"],
    back: ["..IIIIIIII..", "..IBBBBBBI..", "..IBBAABBI..", "..IBBAABBI..", "..IAAAAAAI..", "..IBAAAABI..", "..IBBAABBI..", "..IBBBBBBI..", "..IBBBBBBI..", "..IBBBBBBI..", "..IIIIIIII..", "............"],
    swap: ["............", "IIII....IIII", "IBBI....IBBI", "IBBI..A.IBBI", "IBBIAAAAIBBI", "IBBI..A.IBBI", "IBBI....IBBI", "IBBI....IBBI", "IIII....IIII", "............", "............", "............"],
    book: ["............", "............", ".IIII..IIII.", "IBBBBIIBBBBI", "IBAABIIBAABI", "IBBBBIIBBBBI", "IBAABIIBAABI", "IBBBBIIBBBBI", "IIIIIIIIIIII", "............", "............", "............"],
    wrench: ["........II..", ".......II..I", ".......IIIII", "......IIIII.", ".....AAA....", "....AAA.....", "...AAA......", "..AAA.......", ".AAA........", "AAA.........", "AA..........", "............"],
    lock: ["............", "....IIII....", "...I....I...", "...I....I...", "..IIIIIIII..", "..IAAAAAAI..", "..IAAIIAAI..", "..IAAIIAAI..", "..IAAAAAAI..", "..IIIIIIII..", "............", "............"],
    plug: ["....IIII....", "....I..I....", "...IIIIII...", "...IAAAAI...", "...IAAAAI...", "...IAAAAI...", "....IAAI....", ".....AA.....", ".....AA.....", ".....AA.....", "....AA......", "...AA......."],
    flag: ["..I.........", "..IAAAAAA...", "..IAAAAAAA..", "..IAAAAAA...", "..IAAAAA....", "..I.........", "..I.........", "..I.........", "..I.........", "..I.........", ".IIII.......", "............"],
    check: ["............", "............", "..........AA", ".........AA.", "........AA..", ".AA....AA...", "..AA..AA....", "...AAAA.....", "....AA......", "............", "............", "............"],
    ask: [".IIIIIIIIII.", "IBBBBBBBBBBI", "IBBBAAAABBBI", "IBBAABBAABBI", "IBBBBBBAABBI", "IBBBBBAABBBI", "IBBBBAABBBBI", "IBBBBBBBBBBI", "IBBBBAABBBBI", ".IIIIIIIIII.", "...II.......", "...I........"],
    auto: ["............", "....IIII....", "..II..IIII..", ".I....IIIII.", ".I....IIIII.", "I.....IIIIII", "I.....IIIIII", ".I....IIIII.", ".I....IIIII.", "..II..IIII..", "....IIII....", "............"],
    sun: [".....II.....", ".I........I.", "....IIII....", "...IIIIII...", "...IIIIII...", "II.IIIIII.II", "II.IIIIII.II", "...IIIIII...", "...IIIIII...", "....IIII....", ".I........I.", ".....II....."],
    moon: ["............", "....IIII....", "...III......", "..III.......", "..II........", "..II........", "..III.....I.", "...IIII.III.", "....IIIIII..", "............", "............", "............"],
    pause: ["............", "............", "...II..II...", "...II..II...", "...II..II...", "...II..II...", "...II..II...", "...II..II...", "...II..II...", "...II..II...", "............", "............"],
    cloud: ["................", "......WWWW......", "....WWWWWWWW....", "..WWWWWWWWWWWWW.", ".WWWWWWWWWWWWWWW", "..WWWWWWWWWWWW.."],
    phone: ["IIIIIIIIIIII", "IBBBBBBBBBBI", "IBAAAAAABBBI", "IBBBBBBBBBBI", "IBAAAABBBBBI", "IBBBBBBBBBBI", "IBAAAAAAABBI", "IBBBBBBBBBBI", "IBAAABBBBBBI", "IBBBBBBBBBBI", "IBBBBBBBBBBI", "IBBBBBBBBBBI", "IBBBBBBBBBBI", "IIIIIIIIIIII", "IIIIIBBIIIII", "IIIIIIIIIIII"],
  };

  const rects = (rows, dx = 0, dy = 0) => rows.map((r, y) => [...r].map((c, x) => c === "." ? "" :
    `<rect x="${x + dx}" y="${y + dy}" width="1" height="1" fill="${ink[c]}"/>`).join("")).join("");
  const svg = (box, body) => `<svg viewBox="${box}" shape-rendering="crispEdges" aria-hidden="true" focusable="false">${body}</svg>`;

  // The PC: a case with seven shelves, one per kind of file, as in the app. Each box is a slot that fills in turn.
  function pc() {
    const rows = ["IIIIIIIIIIIIIIIIIIIIIIII"];
    for (let k = 0; k < 7; k++) rows.push("IGGGGGGGGGGGGGGGGGGGGGGI", "IGGGGGGGGGGGGGGGGGGGGGGI", "IIIIIIIIIIIIIIIIIIIIIIII");
    // Per shelf: box width, as the app's loads; videos are crates.
    const slots = [3, 6, 3, 3, 4, 4, 4].flatMap((w, k) => Array.from({ length: Math.floor(21 / (w + 1)) }, (_, i) =>
      `<rect class="slot" x="${2 + i * (w + 1)}" y="${1 + k * 3}" width="${w}" height="2" fill="var(--accent)"/>`)).join("");
    return svg("0 0 24 22", rects(rows) + slots);
  }

  // The courier: every pose is drawn once and CSS shows the parts for the current one (see "Courier" in site.css).
  function courier(front) {
    const legs = (front ? [0, 3, 0, 3] : [0, 2, 0, 1]).map(i => `<g>${rects(LEGS[i], 0, 14)}</g>`).join("");
    return svg("-1 0 10 16",
      `<g class="c-load"><rect x="1" y="0" width="6" height="5" fill="var(--accent)"/><rect x="1" y="2" width="6" height="1" fill="var(--deep)"/>` +
      `<rect x="0" y="5" width="1" height="1" fill="var(--skin)"/><rect x="7" y="5" width="1" height="1" fill="var(--skin)"/></g>` +
      `<g class="c-side">${rects(HEAD, 0, 6)}</g><g class="c-front">${rects(FRONT, 0, 6)}</g>` +
      `<g class="c-up">${rects(ARMS_UP, 0, 11)}</g><g class="c-body">${rects(BODY, 0, 11)}</g><g class="c-out">${rects(ARM_OUT, 0, 11)}</g>` +
      `<rect class="c-wave" x="8" y="9" width="1" height="2" fill="var(--skin)"/><g class="c-legs">${legs}</g>`);
  }

  for (const el of document.querySelectorAll("[data-px]")) {
    const k = el.dataset.px, rows = ART[k];
    el.innerHTML = k === "pc" ? pc() : k === "courier" ? courier(el.classList.contains("front"))
      : svg(`0 0 ${rows[0].length} ${rows.length}`, rects(rows));
  }

  const still = matchMedia("(prefers-reduced-motion: reduce)");

  // Home: each time the courier reaches the PC he leaves a box on the next empty slot; once the PC is full, it starts again.
  const walker = document.querySelector(".walker");
  if (walker) {
    const slots = [...document.querySelectorAll(".world .slot")];
    let n = Math.round(slots.length * 0.4);
    const show = () => slots.forEach((s, i) => s.style.opacity = i < n ? 1 : 0);
    show();
    walker.addEventListener("animationiteration", e => {
      if (e.animationName !== "route") return;
      n = n >= slots.length ? 3 : n + 1;
      show();
    });
    // Moving for more than five seconds, so it gets a pause button.
    const pause = document.querySelector(".world .pause");
    pause.addEventListener("click", () => {
      const on = pause.getAttribute("aria-pressed") !== "true";
      pause.setAttribute("aria-pressed", on);
      walker.parentElement.classList.toggle("paused", on);
    });
    // Click him and he waves, as in the app.
    walker.addEventListener("click", () => {
      if (still.matches || walker.classList.contains("waving")) return;
      walker.classList.add("waving");
      setTimeout(() => walker.classList.remove("waving"), 1200);
    });
  }

  // Home: on the page's cable the courier walks while the page scrolls and stands when it stops.
  const rider = document.querySelector(".rider");
  if (rider) {
    let idle;
    addEventListener("scroll", () => {
      if (still.matches) return;
      rider.classList.add("walk");
      clearTimeout(idle);
      idle = setTimeout(() => rider.classList.remove("walk"), 150);
    }, { passive: true });
  }
})();
