/**
 * Kraken TUI - v2 Capability Showcase
 *
 * Representative sample of the current project surface:
 * - JSX + signal-driven reconciler
 * - Input, Select, ScrollBox, TextArea widgets
 * - Accessibility roles/labels + accessibility events
 * - Runtime theme switching (built-in + custom per-NodeType defaults)
 * - Animation primitives, chaining, choreography groups, position animation
 * - Runtime tree mutation with insertChild() and destroySubtree()
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/showcase.ts
 *
 * Controls:
 *   Esc / q  Quit
 *   Space    Replay hero choreography
 *   t        Cycle theme
 *   b        Toggle runtime banner (insertChild/destroySubtree)
 *   w        Toggle TextArea wrap
 *   Tab      Focus traversal (also emits accessibility events)
 */

import { Buffer } from "buffer";
import {
  Kraken,
  Theme,
  Box,
  Text,
  signal,
  computed,
  render,
  createLoop,
  KeyCode,
  AccessibilityRole,
} from "../ts/src/index";
import { jsx, jsxs } from "../ts/src/jsx/jsx-runtime";
import { ffi } from "../ts/src/ffi";
import type { KrakenEvent } from "../ts/src/index";
import type { Widget } from "../ts/src/widget";

interface ThemeMode {
  name: string;
  theme: Theme;
  accent: string;
  panelBg: string;
  note: string;
  destroyOnExit: boolean;
}

const statusText = signal("Booting showcase...");
const metricsText = signal("metrics: initializing");
const logText = signal("");
const spinnerGlyph = signal("|");
const activeThemeName = signal("Builtin Dark");
const accentColor = signal("#60a5fa");
const rootBackground = signal("#0b1220");
const codeBgColor = signal("#0f172a");
const codeFgColor = signal("#dbeafe");
const hintColor = signal("#94a3b8");
const wrapEnabled = signal(true);
const notesMeta = signal("TextArea lines: 0 | wrap: on");
const runtimeHostHint = signal(
  "Press [b] to insert/remove a runtime subtree (insertChild/destroySubtree).",
);
const footerHint = signal(
  "Esc quit/edit-exit | Space replay anim | t theme | b banner | w wrap | / focus input | n focus notes",
);
const logLineCount = computed(() =>
  Math.max(1, logText.value === "" ? 1 : logText.value.split("\n").length),
);

const logLines: string[] = [];
const textareaSeed = [
  "TextArea wrap/unwrap demo:",
  "1) This line is intentionally long so wrap changes are obvious immediately when you press [w] to toggle soft wrapping on and off in place.",
  "2) SuperLongTokenForWrapTestingWithoutSpaces_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789_repeat_repeat_repeat_repeat",
  "3) Keep typing below this text to confirm cursor movement, editing, and multi-line behavior remain stable while wrap mode changes.",
].join("\n");

function pushLog(message: string): void {
  const stamp = new Date().toISOString().slice(11, 19);
  logLines.push(`${stamp} ${message}`);
  if (logLines.length > 140) {
    logLines.splice(0, logLines.length - 140);
  }
  logText.value = logLines.join("\n");
}

function getContent(handle: number): string {
  const len = ffi.tui_get_content_len(handle);
  if (len <= 0) return "";
  const buf = Buffer.alloc(len + 1);
  const written = ffi.tui_get_content(handle, buf, len + 1);
  if (written <= 0) return "";
  return buf.toString("utf-8", 0, written);
}

function setContent(handle: number, value: string): void {
  const encoded = new TextEncoder().encode(value);
  const buf = Buffer.from(encoded);
  ffi.tui_set_content(handle, buf, encoded.length);
}

function getSelectOption(handle: number, index: number): string {
  const buf = Buffer.alloc(256);
  const written = ffi.tui_select_get_option(handle, index, buf, 256);
  if (written <= 0) return "";
  return buf.toString("utf-8", 0, written);
}

function createAuroraTheme(): Theme {
  const theme = Theme.create();
  theme.setBackground("#031420");
  theme.setForeground("#d8f7ff");
  theme.setBorderColor("#115e59");
  theme.setTypeColor("text", "fg", "#d8f7ff");
  theme.setTypeColor("input", "fg", "#ecfeff");
  theme.setTypeColor("input", "bg", "#083344");
  theme.setTypeBorderStyle("input", "rounded");
  theme.setTypeColor("textarea", "fg", "#cffafe");
  theme.setTypeColor("textarea", "bg", "#164e63");
  theme.setTypeBorderStyle("textarea", "single");
  theme.setTypeColor("select", "fg", "#ecfeff");
  theme.setTypeColor("select", "bg", "#155e75");
  theme.setTypeColor("scrollBox", "borderColor", "#22d3ee");
  return theme;
}

function createSunsetTheme(): Theme {
  const theme = Theme.create();
  theme.setBackground("#1b0f08");
  theme.setForeground("#ffe7d6");
  theme.setBorderColor("#c2410c");
  theme.setTypeColor("text", "fg", "#ffe7d6");
  theme.setTypeColor("input", "fg", "#fff7ed");
  theme.setTypeColor("input", "bg", "#7c2d12");
  theme.setTypeBorderStyle("input", "rounded");
  theme.setTypeColor("textarea", "fg", "#ffedd5");
  theme.setTypeColor("textarea", "bg", "#9a3412");
  theme.setTypeBorderStyle("textarea", "single");
  theme.setTypeColor("select", "fg", "#fff7ed");
  theme.setTypeColor("select", "bg", "#9a3412");
  theme.setTypeColor("scrollBox", "borderColor", "#fb923c");
  return theme;
}

const app = Kraken.init();
const initialTerminalSize = app.getTerminalSize();
const compactLayout =
  initialTerminalSize.width <= 80 || initialTerminalSize.height <= 24;

const auroraTheme = createAuroraTheme();
const sunsetTheme = createSunsetTheme();

function normalizeThemeForShowcase(theme: Theme): void {
  for (const nodeType of [
    "box",
    "text",
    "input",
    "select",
    "scrollBox",
    "textarea",
  ] as const) {
    theme.setTypeBorderStyle(nodeType, "none");
  }
}

const themeModes: ThemeMode[] = [
  {
    name: "Builtin Dark",
    theme: Theme.dark(),
    accent: "#60a5fa",
    panelBg: "#0b1220",
    note: "Theme.dark()",
    destroyOnExit: false,
  },
  {
    name: "Builtin Light",
    theme: Theme.light(),
    accent: "#2563eb",
    panelBg: "#e2e8f0",
    note: "Theme.light()",
    destroyOnExit: false,
  },
  {
    name: "Aurora Custom",
    theme: auroraTheme,
    accent: "#22d3ee",
    panelBg: "#05202b",
    note: "Theme.create() + setType*",
    destroyOnExit: true,
  },
  {
    name: "Sunset Custom",
    theme: sunsetTheme,
    accent: "#fb923c",
    panelBg: "#2b1308",
    note: "Theme.create() + setType*",
    destroyOnExit: true,
  },
];

for (const mode of themeModes) {
  normalizeThemeForShowcase(mode.theme);
}

const ROLE_NAMES: Record<number, string> = {
  [AccessibilityRole.Button]: "button",
  [AccessibilityRole.Checkbox]: "checkbox",
  [AccessibilityRole.Input]: "input",
  [AccessibilityRole.TextArea]: "textarea",
  [AccessibilityRole.List]: "list",
  [AccessibilityRole.ListItem]: "listitem",
  [AccessibilityRole.Heading]: "heading",
  [AccessibilityRole.Region]: "region",
  [AccessibilityRole.Status]: "status",
};

let rootWidget: Widget | null = null;
let heroCard: Widget | null = null;
let liveBadge: Widget | null = null;
let runtimeHost: Widget | null = null;
let runtimeBanner: Box | null = null;
let runtimeBannerLabel: Text | null = null;
let commandInputHandle = 0;
let themeSelectHandle = 0;
let notesHandle = 0;
let currentThemeIndex = 0;
let activeChoreoGroup = 0;
let heroAnimationHandles: number[] = [];
let heroRaised = false;
let spinnerFrame = 0;
let spinnerLastTickMs = Date.now();
const SPINNER_FRAMES = ["|", "/", "-", "\\"] as const;

function updateNotesMeta(): void {
  const lineCount =
    notesHandle === 0
      ? 0
      : Math.max(0, ffi.tui_textarea_get_line_count(notesHandle));
  notesMeta.value = `TextArea lines: ${lineCount} | wrap: ${wrapEnabled.value ? "on" : "off"}`;
}

function applyTheme(index: number): void {
  if (!rootWidget) return;
  const mode = themeModes[index];
  if (!mode) return;

  currentThemeIndex = index;
  activeThemeName.value = mode.name;
  accentColor.value = mode.accent;
  rootBackground.value = mode.panelBg;

  if (index === 1) {
    codeBgColor.value = "#0f172a";
    codeFgColor.value = "#dbeafe";
    hintColor.value = "#334155";
  } else if (index === 2) {
    codeBgColor.value = "#042f2e";
    codeFgColor.value = "#ccfbf1";
    hintColor.value = "#67e8f9";
  } else if (index === 3) {
    codeBgColor.value = "#431407";
    codeFgColor.value = "#ffedd5";
    hintColor.value = "#fdba74";
  } else {
    codeBgColor.value = "#0f172a";
    codeFgColor.value = "#dbeafe";
    hintColor.value = "#94a3b8";
  }

  app.switchTheme(mode.theme);
  if (themeSelectHandle !== 0) {
    ffi.tui_select_set_selected(themeSelectHandle, index);
  }
  applyRuntimeBannerPalette(index);
  statusText.value = `Theme -> ${mode.name} (${mode.note})`;
  pushLog(`theme switched to ${mode.name}`);
}

function cycleTheme(): void {
  const next = (currentThemeIndex + 1) % themeModes.length;
  applyTheme(next);
}

function stopHeroMotion(): void {
  if (heroCard) {
    for (const handle of heroAnimationHandles) {
      try {
        heroCard.cancelAnimation(handle);
      } catch {
        // Ignore cancellation failures for completed animations.
      }
    }
  }
  heroAnimationHandles = [];

  if (activeChoreoGroup !== 0) {
    try {
      app.destroyChoreoGroup(activeChoreoGroup);
    } catch {
      // Ignore invalid or already-destroyed groups.
    }
    activeChoreoGroup = 0;
  }
}

function runHeroChoreography(): void {
  if (!heroCard) return;

  stopHeroMotion();

  const nextOpacity = heroRaised ? 1 : 0.82;
  const borderTarget = heroRaised ? accentColor.value : "#fbbf24";
  heroRaised = !heroRaised;

  const borderPulse = heroCard.animate({
    property: "borderColor",
    target: borderTarget,
    duration: 240,
    easing: "cubicOut",
  });
  const fade = heroCard.animate({
    property: "opacity",
    target: nextOpacity,
    duration: 280,
    easing: "easeInOut",
  });

  app.chainAnimation(borderPulse, fade);

  const badgeTintTarget = heroRaised ? "#f59e0b" : accentColor.value;
  let badgeTint = 0;
  if (liveBadge) {
    badgeTint = liveBadge.animate({
      property: "fgColor",
      target: badgeTintTarget,
      duration: 320,
      easing: "elastic",
    });
  }

  const group = app.createChoreoGroup();
  app.choreoAdd(group, borderPulse, 0);
  if (badgeTint !== 0) {
    app.choreoAdd(group, badgeTint, 90);
  }
  app.startChoreo(group);

  activeChoreoGroup = group;
  heroAnimationHandles =
    badgeTint !== 0 ? [borderPulse, fade, badgeTint] : [borderPulse, fade];
  statusText.value = "Animation choreography replayed";
  pushLog("hero choreography started");
}

function applyRuntimeBannerPalette(index: number): void {
  if (!runtimeBanner) return;

  if (index === 1) {
    runtimeBanner.setForeground("#1d4ed8");
    runtimeBanner.setBackground("#dbeafe");
    runtimeBannerLabel?.setForeground("#1e3a8a");
    return;
  }

  if (index === 2) {
    runtimeBanner.setForeground("#22d3ee");
    runtimeBanner.setBackground("#083344");
    runtimeBannerLabel?.setForeground("#cffafe");
    return;
  }

  if (index === 3) {
    runtimeBanner.setForeground("#fb923c");
    runtimeBanner.setBackground("#7c2d12");
    runtimeBannerLabel?.setForeground("#ffedd5");
    return;
  }

  runtimeBanner.setForeground("#67e8f9");
  runtimeBanner.setBackground("#082f49");
  runtimeBannerLabel?.setForeground("#cffafe");
}

function toggleRuntimeBanner(): void {
  if (!runtimeHost) return;

  if (runtimeBanner) {
    runtimeBanner.destroySubtree();
    runtimeBanner = null;
    runtimeBannerLabel = null;
    runtimeHostHint.value =
      "Press [b] to insert/remove a runtime subtree (insertChild/destroySubtree).";
    statusText.value = "Runtime subtree destroyed via destroySubtree()";
    pushLog("runtime subtree destroyed");
    return;
  }

  const banner = new Box({
    width: "100%",
    height: 1,
    flexDirection: "row",
    alignItems: "center",
    padding: [0, 0, 0, 0],
    border: "none",
    fg: "#67e8f9",
    bg: "#082f49",
  });
  banner.setRole(AccessibilityRole.Region);
  banner.setLabel("Runtime mutation banner");
  banner.setDescription(
    "Inserted at runtime using insertChild and removable via destroySubtree",
  );

  const label = new Text({
    content:
      "Runtime subtree inserted at index 0. Press b again to destroy it.",
    fg: "#cffafe",
    bold: true,
  });
  label.setRole(AccessibilityRole.Status);
  label.setLabel("Runtime subtree status");
  label.setWidth("100%");
  label.setHeight(1);

  banner.append(label);
  runtimeHost.insertChild(banner, 0);
  runtimeBanner = banner;
  runtimeBannerLabel = label;
  applyRuntimeBannerPalette(currentThemeIndex);
  runtimeHostHint.value = "Subtree mounted. Press [b] again to destroy it.";
  statusText.value = "Runtime subtree inserted via insertChild(index=0)";
  pushLog("runtime subtree inserted at index 0");

  const slideIn = banner.animate({
    property: "positionX",
    target: 2,
    duration: 220,
    easing: "cubicOut",
  });
  const fade = banner.animate({
    property: "opacity",
    target: 0.75,
    duration: 240,
    easing: "easeOut",
  });
  app.chainAnimation(slideIn, fade);
}

function toggleWrap(): void {
  wrapEnabled.value = !wrapEnabled.value;
  statusText.value = `TextArea wrap ${wrapEnabled.value ? "enabled" : "disabled"}`;
  pushLog(`textarea wrap set to ${wrapEnabled.value ? "on" : "off"}`);
  updateNotesMeta();
}

function runCommand(raw: string): void {
  const command = raw.trim().toLowerCase();
  if (command.length === 0) {
    statusText.value = "Input submitted with empty command";
    return;
  }

  switch (command) {
    case "anim":
    case "animate":
      runHeroChoreography();
      return;
    case "theme":
    case "theme next":
      cycleTheme();
      return;
    case "banner":
      toggleRuntimeBanner();
      return;
    case "wrap":
      toggleWrap();
      return;
    default:
      if (command.startsWith("theme ")) {
        const index = Number(command.slice(6).trim());
        if (
          Number.isInteger(index) &&
          index >= 0 &&
          index < themeModes.length
        ) {
          applyTheme(index);
          return;
        }
      }
      statusText.value = `Input command: ${raw}`;
      pushLog(`command submitted: ${raw}`);
  }
}

function handleThemeSelection(event: KrakenEvent): void {
  const fromEvent = event.selectedIndex;
  const selected =
    fromEvent ??
    (event.target !== 0 ? ffi.tui_select_get_selected(event.target) : -1);
  if (selected < 0 || selected >= themeModes.length) return;
  applyTheme(selected);
  pushLog(`select changed to: ${getSelectOption(event.target, selected)}`);
}

function handleCommandSubmit(event: KrakenEvent): void {
  const value = getContent(event.target);
  runCommand(value);
  setContent(event.target, "");
}

function handleTextAreaChange(): void {
  updateNotesMeta();
}

const tree = jsxs("Box", {
  width: "100%",
  height: "100%",
  flexDirection: "column",
  padding: 1,
  gap: 1,
  bg: rootBackground,
  role: "region",
  "aria-label": "Kraken v2 capability showcase",
  children: [
    jsx("Text", {
      key: "header",
      content:
        "# Kraken TUI v2 Showcase\n\nSignals + JSX + native FFI engine in one interactive sample.",
      format: "markdown",
      fg: accentColor,
      height: 4,
      role: "heading",
      "aria-label": "Kraken showcase title",
    }),
    jsxs("Box", {
      key: "main",
      width: "100%",
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      flexDirection: "row",
      gap: 1,
      children: [
        jsxs("Box", {
          key: "control-deck",
          width: "42%",
          border: "rounded",
          padding: compactLayout ? 0 : 1,
          gap: compactLayout ? 0 : 1,
          flexDirection: "column",
          flexGrow: 1,
          flexShrink: 1,
          flexBasis: 0,
          role: "region",
          "aria-label": "Control deck",
          children: [
            jsx("Text", {
              key: "control-title",
              content: "Control Deck",
              bold: true,
              fg: accentColor,
              height: 1,
            }),
            jsx("Input", {
              key: "command-input",
              width: "100%",
              height: 2,
              border: "single",
              focusable: true,
              role: "input",
              "aria-label": "Command input",
              "aria-description":
                "Enter anim, banner, wrap, or theme and press Enter",
              onSubmit: handleCommandSubmit,
              ref: (w: Widget) => {
                commandInputHandle = w.handle;
              },
            }),
            jsx("Select", {
              key: "theme-select",
              options: themeModes.map((mode) => mode.name),
              width: "100%",
              height: compactLayout ? 2 : 3,
              border: "single",
              focusable: true,
              role: "list",
              "aria-label": "Theme selector",
              "aria-description":
                "Arrow keys update theme defaults across the subtree",
              onChange: handleThemeSelection,
              onSubmit: handleThemeSelection,
              ref: (w: Widget) => {
                themeSelectHandle = w.handle;
              },
            }),
            jsx("TextArea", {
              key: "notes",
              value: textareaSeed,
              wrap: wrapEnabled,
              width: "100%",
              height: compactLayout ? 2 : 3,
              border: "single",
              focusable: true,
              role: "textarea",
              "aria-label": "Notes editor",
              "aria-description": "Editable multiline text area",
              onChange: handleTextAreaChange,
              ref: (w: Widget) => {
                notesHandle = w.handle;
              },
            }),
            jsx("Text", {
              key: "notes-meta",
              content: notesMeta,
              height: 1,
            }),
            jsx("Text", {
              key: "code",
              content: [
                'pub extern "C" fn tui_render() -> i32 {',
                "    ffi_wrap(|| render::render(&mut ctx).map(|_| 0))",
                "}",
              ].join("\n"),
              format: "code",
              language: "rust",
              border: "single",
              fg: codeFgColor,
              bg: codeBgColor,
              height: compactLayout ? 1 : 2,
              role: "status",
              "aria-label": "Syntax highlighted Rust snippet",
            }),
          ],
        }),
        jsxs("Box", {
          key: "observability",
          width: "58%",
          flexDirection: "column",
          flexGrow: 1,
          flexShrink: 1,
          flexBasis: 0,
          gap: compactLayout ? 0 : 1,
          children: [
            jsxs("Box", {
              key: "hero",
              border: "rounded",
              padding: compactLayout ? 0 : 1,
              gap: compactLayout ? 0 : 1,
              height: compactLayout ? 6 : 11,
              flexDirection: "column",
              flexShrink: 0,
              role: "status",
              "aria-label": "Live metrics card",
              ref: (w: Widget) => {
                heroCard = w;
              },
              children: [
                jsxs("Box", {
                  key: "badge-row",
                  width: "100%",
                  height: 1,
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 1,
                  children: [
                    jsx("Text", {
                      key: "badge-label",
                      content: "native core live",
                      width: 18,
                      bold: true,
                      fg: accentColor,
                      height: 1,
                      role: "status",
                      "aria-label": "Native core live badge",
                      ref: (w: Widget) => {
                        liveBadge = w;
                      },
                    }),
                    jsx("Text", {
                      key: "badge-spinner",
                      content: spinnerGlyph,
                      width: 1,
                      fg: accentColor,
                      height: 1,
                      role: "status",
                      "aria-label": "Live spinner glyph",
                    }),
                  ],
                }),
                jsx("Text", {
                  key: "theme",
                  content: activeThemeName,
                  fg: accentColor,
                  bold: true,
                  height: 1,
                  role: "status",
                  "aria-label": "Active theme",
                }),
                jsx("Text", {
                  key: "metrics",
                  content: metricsText,
                  height: 1,
                  role: "status",
                  "aria-label": "Runtime metrics",
                }),
                jsx("Text", {
                  key: "status",
                  content: statusText,
                  height: 1,
                  role: "status",
                  "aria-label": "Action status",
                }),
              ],
            }),
            jsxs("Box", {
              key: "runtime-host",
              border: "single",
              padding: compactLayout ? 0 : [0, 1, 0, 1],
              height: 4,
              flexDirection: "column",
              flexShrink: 0,
              role: "region",
              "aria-label": "Runtime tree operations host",
              ref: (w: Widget) => {
                runtimeHost = w;
              },
              children: [
                jsx("Text", {
                  key: "runtime-title",
                  content: "Runtime Tree Ops [b] insert/remove subtree",
                  fg: accentColor,
                  height: 1,
                  role: "status",
                  "aria-label": "Runtime host title",
                }),
                jsx("Text", {
                  key: "runtime-label",
                  content: runtimeHostHint,
                  fg: hintColor,
                  height: 1,
                  role: "status",
                  "aria-label": "Runtime host status",
                }),
              ],
            }),
            jsxs("ScrollBox", {
              key: "log-scroll",
              width: "100%",
              flexGrow: 1,
              flexShrink: 1,
              flexBasis: 0,
              border: "single",
              role: "list",
              "aria-label": "Event log",
              children: [
                jsx("Text", {
                  key: "log-text",
                  content: logText,
                  width: "100%",
                  height: logLineCount,
                  role: "status",
                  "aria-label": "Log output",
                }),
              ],
            }),
          ],
        }),
      ],
    }),
    jsx("Text", {
      key: "footer",
      content: footerHint,
      fg: accentColor,
      height: 1,
      role: "status",
      "aria-label": "Keyboard help",
    }),
  ],
});

const instance = render(tree, app);
rootWidget = instance.widget;

if (themeSelectHandle !== 0) {
  ffi.tui_focus(themeSelectHandle);
}

applyTheme(0);
updateNotesMeta();
pushLog("showcase initialized");
runHeroChoreography();

const loop = createLoop({
  app,
  onEvent(event: KrakenEvent) {
    if (event.type === "key") {
      const focused = ffi.tui_get_focused();
      const editingText =
        focused === commandInputHandle || focused === notesHandle;
      if (event.keyCode === KeyCode.Escape) {
        if (
          editingText && themeSelectHandle !== 0
        ) {
          ffi.tui_focus(themeSelectHandle);
          pushLog("left text-edit mode");
          return;
        }
        loop.stop();
        return;
      }

      const cp = event.codepoint ?? 0;
      if (cp === 0) return;
      if (editingText) return;
      const key = String.fromCodePoint(cp).toLowerCase();
      if (key === "q") {
        loop.stop();
        return;
      }
      if (key === "/") {
        if (commandInputHandle !== 0) ffi.tui_focus(commandInputHandle);
        return;
      }
      if (key === "n") {
        if (notesHandle !== 0) ffi.tui_focus(notesHandle);
        return;
      }
      if (key === " ") {
        runHeroChoreography();
      }
      if (key === "t") {
        cycleTheme();
      }
      if (key === "b") {
        toggleRuntimeBanner();
      }
      if (key === "w") {
        toggleWrap();
      }
    }

    if (event.type === "focus") {
      pushLog(`focus ${event.fromHandle ?? 0} -> ${event.toHandle ?? 0}`);
    }

    if (event.type === "accessibility") {
      const roleCode = event.roleCode ?? 0;
      const roleName =
        roleCode === 0xffffffff
          ? "label-only"
          : (ROLE_NAMES[roleCode] ?? `unknown(${roleCode})`);
      statusText.value = `Accessibility event -> handle=${event.target}, role=${roleName}`;
      pushLog(`accessibility target=${event.target} role=${roleName}`);
    }
  },
  onTick() {
    const now = Date.now();
    if (now - spinnerLastTickMs >= 90) {
      spinnerFrame = (spinnerFrame + 1) % SPINNER_FRAMES.length;
      spinnerGlyph.value = SPINNER_FRAMES[spinnerFrame]!;
      spinnerLastTickMs = now;
    }

    const activeAnimations = Number(app.getPerfCounter(6));
    metricsText.value = `nodes=${app.getNodeCount()} anim=${activeAnimations} focus=${app.getFocused()} theme=${activeThemeName.value}`;
  },
});

try {
  await loop.start();
} finally {
  if (runtimeBanner) {
    runtimeBanner.destroySubtree();
    runtimeBanner = null;
  }

  stopHeroMotion();

  for (const mode of themeModes) {
    if (!mode.destroyOnExit) continue;
    try {
      mode.theme.destroy();
    } catch {
      // Ignore teardown issues for already-destroyed handles.
    }
  }

  app.shutdown();
}
