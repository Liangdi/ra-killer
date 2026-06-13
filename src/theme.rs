//! Sci-fi HUD styling layer.
//!
//! All TUI appearance lives here as a single `ratatui-style` CSS stylesheet.
//! [`Theme`] parses it once and exposes semantic accessors that return a
//! [`ComputedStyle`] — callers turn that into a ratatui `Style` via
//! `.to_style()` or a `Block` via `.to_block()`. `main.rs` never constructs a
//! `Color`/`Style` by hand.

use anyhow::Result;
use ratatui_style::{ComputedStyle, OwnedNode, Stylesheet};

/// Neon cyberpunk palette + element rules. Borders carry their color via the
/// shorthand (`border: double var(--cyan)`) so `to_block()` paints neon frames.
pub const SCIFI_CSS: &str = r#"
:root {
    --cyan:    #56f0ff;
    --magenta: #ff4fd8;
    --green:   #5dffb0;
    --amber:   #ffb454;
    --red:     #ff5c6e;
    --bg:      #04111a;
    --panel:   #07222e;
    --dim:     #2a6a7a;
    --track:   #0a1a24;
    --selbg:   #2a0f24;
    --text:    #b8f2ff;
}

/* Full-screen space background. */
Root          { background: var(--bg); }

/* Header band + live status. */
Header        { color: var(--cyan); background: var(--panel); font-weight: bold; }
Subtitle      { color: var(--dim); }
Status        { color: var(--green); font-weight: bold; }
Status.warn   { color: var(--amber); }
Status.alert  { color: var(--magenta); }

/* Framed panels: main = cyan double-line, default = dim double-line. */
Panel         { color: var(--cyan); border: double var(--dim); }
Panel.main    { color: var(--cyan); border: double var(--cyan); }

/* Telemetry label / value pairs. */
Label         { color: var(--dim); }
Value         { color: var(--text); }
Value.ok      { color: var(--green); }
Value.warn    { color: var(--amber); }
Value.alert   { color: var(--magenta); }

/* Block-bar + gauge fill (background = dark track). */
Bar           { color: var(--green);  background: var(--track); }
Bar.warn      { color: var(--amber); }
Bar.alert     { color: var(--magenta); }
BarEmpty      { color: var(--dim); }

/* Process table. */
ThCell        { color: var(--cyan); font-weight: bold; }
Row           { color: var(--text); }
Row.selected  { color: var(--magenta); background: var(--selbg); font-weight: bold; }

/* Footer / help / dialogs. */
Key           { color: var(--cyan); font-weight: bold; }
Hint          { color: var(--dim); }
Footer        { color: var(--dim); border: rounded var(--dim); }
Confirm       { color: var(--amber); font-weight: bold; }
Msg           { color: var(--text); background: var(--panel); border: double var(--cyan); }
"#;

/// Memory-pressure level, mapped to a CSS class (`ok` / `warn` / `alert`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoadLevel {
    Normal,
    Warn,
    Alert,
}

impl LoadLevel {
    /// Alert at/over `threshold`, Warn from 80% of it, else Normal.
    pub fn from_percent(pct: f64, threshold: u8) -> Self {
        let t = threshold as f64;
        if pct >= t {
            Self::Alert
        } else if pct >= t * 0.8 {
            Self::Warn
        } else {
            Self::Normal
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Normal => "ok",
            Self::Warn => "warn",
            Self::Alert => "alert",
        }
    }
}

/// Parsed stylesheet wrapped in semantic accessors.
pub struct Theme {
    sheet: Stylesheet,
}

impl Theme {
    /// Parse the sci-fi stylesheet (once, at TUI startup).
    pub fn new() -> Result<Self> {
        Ok(Self {
            sheet: Stylesheet::parse(SCIFI_CSS)?,
        })
    }

    fn compute(&self, type_name: &str, class: Option<&str>) -> ComputedStyle {
        let node = match class {
            Some(c) => OwnedNode::new(type_name).with_classes([c]),
            None => OwnedNode::new(type_name),
        };
        self.sheet.compute(&node, None)
    }

    // --- background / header ---
    pub fn root(&self) -> ComputedStyle {
        self.compute("Root", None)
    }
    pub fn header(&self) -> ComputedStyle {
        self.compute("Header", None)
    }
    pub fn subtitle(&self) -> ComputedStyle {
        self.compute("Subtitle", None)
    }
    pub fn status(&self, level: LoadLevel) -> ComputedStyle {
        let cls = match level {
            LoadLevel::Normal => None,
            LoadLevel::Warn => Some("warn"),
            LoadLevel::Alert => Some("alert"),
        };
        self.compute("Status", cls)
    }

    // --- panels ---
    /// `main = true` → cyan double-line frame; `false` → dim double-line.
    pub fn panel(&self, main: bool) -> ComputedStyle {
        self.compute("Panel", main.then_some("main"))
    }

    // --- telemetry ---
    pub fn label(&self) -> ComputedStyle {
        self.compute("Label", None)
    }
    pub fn value_plain(&self) -> ComputedStyle {
        self.compute("Value", None)
    }
    pub fn value(&self, level: LoadLevel) -> ComputedStyle {
        self.compute("Value", Some(level.class()))
    }
    pub fn bar(&self, level: LoadLevel) -> ComputedStyle {
        self.compute("Bar", Some(level.class()))
    }
    pub fn bar_empty(&self) -> ComputedStyle {
        self.compute("BarEmpty", None)
    }

    // --- process table ---
    pub fn th_cell(&self) -> ComputedStyle {
        self.compute("ThCell", None)
    }
    pub fn row(&self, selected: bool) -> ComputedStyle {
        self.compute("Row", selected.then_some("selected"))
    }

    // --- footer / dialogs ---
    pub fn key(&self) -> ComputedStyle {
        self.compute("Key", None)
    }
    pub fn hint(&self) -> ComputedStyle {
        self.compute("Hint", None)
    }
    pub fn footer(&self) -> ComputedStyle {
        self.compute("Footer", None)
    }
    pub fn confirm(&self) -> ComputedStyle {
        self.compute("Confirm", None)
    }
    pub fn msg(&self) -> ComputedStyle {
        self.compute("Msg", None)
    }
}
