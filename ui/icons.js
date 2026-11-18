// One icon style for the whole app: single-color line icons (24 px grid, 1.75 stroke, currentColor),
// drawn for RigSweep. UI strings keep their emoji (all 12 languages); wherever text that starts with
// one of these emoji lands on the page, the emoji is swapped for its icon. Unknown emoji stay as is.
const ICON_PATHS = {
  home: "M3 11l9-7 9 7M5 9.5V20h14V9.5M10 20v-6h4v6",
  broom: "M20 3l-7.4 7.4M9.6 9.3l5.1 5.1-1.6 2.3C11.7 18.8 9 20 5.5 20H4c0-3.5 1.2-6.2 3.3-7.6zM8 16l-2 2M11 17l-1.5 2",
  zap: "M13 2L4 14h7l-1 8 9-12h-7z",
  monitor: "M5 4h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM8 20h8M12 16v4",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  sliders: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M16 4v4M10 10v4M18 16v4",
  toggle: "M7 7h10a5 5 0 0 1 0 10H7A5 5 0 0 1 7 7zM16 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z",
  bars: "M4 20V10M10 20V4M16 20v-7M3 20h18",
  trend: "M3 17l6-6 4 4 8-8M15 7h6v6",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4",
  rocket: "M12 2.5c3 2 5 5.8 5 9.5l-2 4H9l-2-4c0-3.7 2-7.5 5-9.5zM12 8.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3zM9 16l-2 4 3-1M15 16l2 4-3-1",
  shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z",
  save: "M5 4h11l4 4v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1zM8 4v5h7V4M8 20v-6h8v6",
  drive: "M5 5h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM3 13h18M7 16h.01M11 16h6",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
  thermo: "M14 14.5V5a2 2 0 0 0-4 0v9.5a4 4 0 1 0 4 0zM12 9v6",
  flame: "M12 21c-4 0-6.5-2.7-6.5-6.2 0-2.8 1.8-4.6 3-6.8.6 1.8 1.8 2.8 3 3 0-3 1.2-6 4-8 0 4 4.5 6 4.5 11.3 0 4-3.2 6.7-8 6.7z",
  refresh: "M20 11a8 8 0 0 0-14.6-4.4L4 8M4 4v4h4M4 13a8 8 0 0 0 14.6 4.4L20 16M20 20v-4h-4",
  warn: "M10.3 3.9L2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0zM12 9v4M12 17h.01",
  check: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM8 12.5l2.5 2.5L16 9.5",
  stop: "M8 3h8l5 5v8l-5 5H8l-5-5V8zM12 8v5M12 16h.01",
  lock: "M7 11h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 11V8a4 4 0 0 1 8 0v3",
  bot: "M7 8h10a3 3 0 0 1 3 3v6a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3v-6a3 3 0 0 1 3-3zM12 4v4M9 13h.01M15 13h.01M9.5 17h5",
  moon: "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z",
  plug: "M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4",
  ghost: "M5 21V10a7 7 0 0 1 14 0v11l-2.3-2-2.3 2-2.4-2-2.4 2-2.3-2zM9.5 10h.01M14.5 10h.01",
  terminal: "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM7 9l3 3-3 3M12 15h5",
  server: "M5 4h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM5 13h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2zM7 7.5h.01M7 16.5h.01",
  copy: "M10 8h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2zM16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2",
  sparkle: "M11 3l1.8 4.2L17 9l-4.2 1.8L11 15l-1.8-4.2L5 9l4.2-1.8zM18.5 14l.8 1.7 1.7.8-1.7.8-.8 1.7-.8-1.7-1.7-.8 1.7-.8z",
  trophy: "M8 4h8v5a4 4 0 0 1-8 0zM8 6H5a3 3 0 0 0 3 4M16 6h3a3 3 0 0 1-3 4M12 13v4M8 20h8M10 17h4",
  play: "M7 4.5l12 7.5-12 7.5z",
  square: "M7.5 6h9A1.5 1.5 0 0 1 18 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 6 16.5v-9A1.5 1.5 0 0 1 7.5 6z",
  pause: "M8 5v14M16 5v14",
  camera: "M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1zM12 10a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7z",
  clipboard: "M8 4H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-1M9 2.5h6v3H9zM9 11h6M9 15h4",
  chip: "M4.5 7h15A1.5 1.5 0 0 1 21 8.5v7a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 15.5v-7A1.5 1.5 0 0 1 4.5 7zM7 17v3M12 17v3M17 17v3M7 10.5v3M12 10.5v3M17 10.5v3",
  list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  layers: "M12 3l9 5-9 5-9-5zM3 13l9 5 9-5",
  target: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM12 11.5a.5.5 0 1 0 0 1 .5.5 0 0 0 0-1z",
  up: "M12 19V5M6 11l6-6 6 6",
  cloud: "M7 18a4.5 4.5 0 0 1-.5-9 6 6 0 0 1 11.5 1.5A3.8 3.8 0 0 1 17.5 18z",
  gamepad: "M6.5 8h11a4 4 0 0 1 3.9 4.9l-.9 3.9a2.5 2.5 0 0 1-4.3 1.1L14.7 16H9.3l-1.5 1.9a2.5 2.5 0 0 1-4.3-1.1l-.9-3.9A4 4 0 0 1 6.5 8zM7 12h3M8.5 10.5v3M15.5 12h.01M17.5 12h.01",
  branch: "M6 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 8v8M18 10c0 4-4 5.5-10 7",
  box: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9",
  ban: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM5.6 5.6l12.8 12.8",
  eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
};
const ICON_FOR = {
  "🏠": "home", "🧹": "broom", "⚡": "zap", "🖥": "monitor", "🗂": "folder", "🛠": "sliders", "⚙": "toggle",
  "📊": "bars", "📈": "trend", "🔍": "search", "🚀": "rocket", "🛡": "shield", "💾": "save", "💽": "drive",
  "🌐": "globe", "🌍": "globe", "🌡": "thermo", "🔥": "flame", "🔄": "refresh", "♻": "refresh", "⚠": "warn",
  "✅": "check", "🛑": "stop", "🔒": "lock", "🤖": "bot", "💤": "moon", "🔌": "plug", "🧟": "ghost", "🐚": "terminal",
  "🐧": "server", "👯": "copy", "🎉": "sparkle", "🏆": "trophy", "▶": "play", "⏹": "square", "⏸": "pause",
  "📸": "camera", "📋": "clipboard", "🧠": "chip", "🆓": "chip", "🔢": "list", "🧩": "layers", "🎯": "target",
  "⬆": "up", "☁": "cloud", "🎮": "gamepad", "🌿": "branch", "📦": "box", "🚫": "ban", "👁": "eye",
};
const ICON_LEAD = /^(\s*)(\p{Extended_Pictographic})️?\s?/u;
const ICON_SKIP = new Set(["OPTION", "SELECT", "TEXTAREA", "SCRIPT", "STYLE", "TITLE", "PRE", "CODE"]);

function iconSvg(name) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="${ICON_PATHS[name]}"/></svg>`;
}

function iconizeText(node) {
  const m = node.nodeValue.match(ICON_LEAD);
  const name = m && ICON_FOR[m[2]];
  const parent = node.parentNode;
  if (!name || !parent || ICON_SKIP.has(parent.nodeName) || parent.closest?.("[data-no-icon]")) return;
  const span = document.createElement("span");
  span.className = "ico";
  span.setAttribute("aria-hidden", "true");
  span.innerHTML = iconSvg(name);
  node.nodeValue = node.nodeValue.slice(m[0].length);
  parent.insertBefore(span, node);
}

function iconize(root) {
  if (root.nodeType === Node.TEXT_NODE) return iconizeText(root);
  if (root.nodeType !== Node.ELEMENT_NODE || ICON_SKIP.has(root.nodeName)) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.nodeValue.trimStart().codePointAt(0) > 0x2000 ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP),
  });
  const found = [];
  while (walker.nextNode()) found.push(walker.currentNode);
  found.forEach(iconizeText);
}

// convert what's on the page now, then everything that gets rendered later
iconize(document.body);
new MutationObserver((muts) => {
  for (const m of muts) {
    if (m.type === "characterData") iconizeText(m.target);
    else m.addedNodes.forEach(iconize);
  }
}).observe(document.body, { childList: true, subtree: true, characterData: true });
