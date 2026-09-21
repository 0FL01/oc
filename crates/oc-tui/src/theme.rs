//! Upstream OpenCode v2.0.12 theme palette.
//!
//! The vendored asset is `assets/upstream/v2/opencode.json`, copied
//! byte-for-byte from `packages/tui/src/theme/assets/v2/opencode.json` at tag
//! `v2.0.12` (see `assets/upstream/PROVENANCE.md`). Upstream defaults are
//! theme name `opencode` and mode `dark`
//! (`evidence/tui/upstream-inventory.md` §2).
//!
//! # Resolution model
//!
//! Mirrors `packages/theme/src/tui/{select,resolve}.ts`:
//!
//! - the `base` token tree is merged with the selected mode overrides
//!   (`dark`/`light`, objects merged recursively, arrays/scalars replaced);
//! - `$ref` strings resolve by dotted path inside that merged tree;
//! - `$hue.<name>.<step>` resolves through the mode hue scales, including
//!   aliases (`yellow`, `accent`, `interactive`, `neutral`) which reference
//!   another scale;
//! - `transparent` keeps alpha 0; numbers (ANSI indexes, v1-only) and other
//!   strings are rejected rather than approximated;
//! - the `@dialog` surface is part of the same tree, so its slots are
//!   reachable as `@dialog.…`.
//!
//! Every resolved leaf is stored in [`Theme::palette`] under its dotted asset
//! path, so no slot is silently dropped. State keys keep the asset's `$`
//! prefix (`text.action.primary.$focused`).
//!
//! # Alpha approximation
//!
//! Upstream allows RGBA. Ratatui has no alpha compositing, so values keep
//! their alpha in [`Rgba`] and convert to [`ratatui::style::Color`] with
//! [`Rgba::to_color`], which composites straight-alpha over the resolved
//! `background.base` (itself composited over opaque black). Upstream paints
//! the whole frame with `theme.background.base`, so for a truecolor terminal
//! this opaque approximation is pixel-equivalent inside the root frame. The
//! vendored asset has no alpha values; the `#rgba`/`#rrggbbaa` forms are
//! supported and covered by unit tests.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use ratatui::style::Color;
use serde_json::Value;

/// Vendored, byte-faithful copy of the upstream default theme asset.
pub const OPENCODE_THEME_JSON: &str = include_str!("../assets/upstream/v2/opencode.json");

/// Upstream hue scale names plus aliases (`schema.ts` `BaseHue`/`HueAlias`).
const HUE_NAMES: [&str; 11] = [
    "gray",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "purple",
    "accent",
    "interactive",
    "neutral",
];

/// Every hue scale has exactly these nine steps (`schema.ts` `HueStep`).
const HUE_STEPS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// Theme mode selected from the asset (`dark` is the upstream default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    const fn key(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

/// A resolved theme value: straight-alpha RGB, or fully transparent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Fully transparent; upstream `transparent`.
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Opaque black, the alpha underlay root.
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (upstream `RGBA.fromHex`).
    pub fn from_hex(value: &str) -> Option<Self> {
        let digits = value.strip_prefix('#')?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let nibble = |index: usize| u8::from_str_radix(&digits[index..=index], 16).ok();
        let byte = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
        match digits.len() {
            3 => Some(Self::new(
                nibble(0)? * 17,
                nibble(1)? * 17,
                nibble(2)? * 17,
                255,
            )),
            4 => Some(Self::new(
                nibble(0)? * 17,
                nibble(1)? * 17,
                nibble(2)? * 17,
                nibble(3)? * 17,
            )),
            6 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Straight-alpha source-over compositing of `self` on top of `under`.
    pub fn over(self, under: Rgba) -> Rgba {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return under;
        }
        let source = u32::from(self.a);
        let under_alpha = u32::from(under.a);
        let out_alpha = source + under_alpha * (255 - source) / 255;
        let blend = |src: u8, dst: u8| -> u8 {
            ((u32::from(src) * source + u32::from(dst) * under_alpha * (255 - source) / 255)
                / out_alpha) as u8
        };
        Rgba {
            r: blend(self.r, under.r),
            g: blend(self.g, under.g),
            b: blend(self.b, under.b),
            a: out_alpha as u8,
        }
    }

    /// Opaque truecolor approximation: composite over `under` (must be opaque)
    /// and drop alpha, which a plain terminal cannot render.
    pub fn to_color(self, under: Rgba) -> Color {
        let out = self.over(under);
        Color::Rgb(out.r, out.g, out.b)
    }
}

/// Syntax token names (`packages/theme/src/tui/schema.ts` `SyntaxToken`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxToken {
    Comment,
    Keyword,
    Function,
    Variable,
    String,
    Number,
    Type,
    Operator,
    Punctuation,
}

impl SyntaxToken {
    pub const ALL: [SyntaxToken; 9] = [
        SyntaxToken::Comment,
        SyntaxToken::Keyword,
        SyntaxToken::Function,
        SyntaxToken::Variable,
        SyntaxToken::String,
        SyntaxToken::Number,
        SyntaxToken::Type,
        SyntaxToken::Operator,
        SyntaxToken::Punctuation,
    ];

    pub const fn path(self) -> &'static str {
        match self {
            SyntaxToken::Comment => "syntax.comment",
            SyntaxToken::Keyword => "syntax.keyword",
            SyntaxToken::Function => "syntax.function",
            SyntaxToken::Variable => "syntax.variable",
            SyntaxToken::String => "syntax.string",
            SyntaxToken::Number => "syntax.number",
            SyntaxToken::Type => "syntax.type",
            SyntaxToken::Operator => "syntax.operator",
            SyntaxToken::Punctuation => "syntax.punctuation",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Markdown token names (`packages/theme/src/tui/schema.ts` `MarkdownToken`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownToken {
    Text,
    Heading,
    Link,
    LinkText,
    Code,
    BlockQuote,
    Emphasis,
    Strong,
    HorizontalRule,
    ListItem,
    ListEnumeration,
    Image,
    ImageText,
    CodeBlock,
}

impl MarkdownToken {
    pub const ALL: [MarkdownToken; 14] = [
        MarkdownToken::Text,
        MarkdownToken::Heading,
        MarkdownToken::Link,
        MarkdownToken::LinkText,
        MarkdownToken::Code,
        MarkdownToken::BlockQuote,
        MarkdownToken::Emphasis,
        MarkdownToken::Strong,
        MarkdownToken::HorizontalRule,
        MarkdownToken::ListItem,
        MarkdownToken::ListEnumeration,
        MarkdownToken::Image,
        MarkdownToken::ImageText,
        MarkdownToken::CodeBlock,
    ];

    pub const fn path(self) -> &'static str {
        match self {
            MarkdownToken::Text => "markdown.text",
            MarkdownToken::Heading => "markdown.heading",
            MarkdownToken::Link => "markdown.link",
            MarkdownToken::LinkText => "markdown.linkText",
            MarkdownToken::Code => "markdown.code",
            MarkdownToken::BlockQuote => "markdown.blockQuote",
            MarkdownToken::Emphasis => "markdown.emphasis",
            MarkdownToken::Strong => "markdown.strong",
            MarkdownToken::HorizontalRule => "markdown.horizontalRule",
            MarkdownToken::ListItem => "markdown.listItem",
            MarkdownToken::ListEnumeration => "markdown.listEnumeration",
            MarkdownToken::Image => "markdown.image",
            MarkdownToken::ImageText => "markdown.imageText",
            MarkdownToken::CodeBlock => "markdown.codeBlock",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Everything that can be wrong with a theme asset; there are no silent
/// defaults, a malformed asset or a missing role is a hard error.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("theme asset is not valid JSON: {0}")]
    Json(serde_json::Error),
    #[error("theme asset root must be a JSON object")]
    RootNotObject,
    #[error("theme asset is missing the `{0}` section")]
    MissingSection(&'static str),
    #[error("theme asset declares unknown hue `{0}`")]
    UnknownHue(String),
    #[error("hue `{name}` is missing step `{step}`")]
    MissingHueStep { name: String, step: u16 },
    #[error("hue `{name}` has an invalid step `{step}`")]
    InvalidHueStep { name: String, step: String },
    #[error("hue alias at `{path}` must reference another `$hue.<name>` scale, got `{value}`")]
    InvalidHueAlias { path: String, value: String },
    #[error("invalid color `{value}` at `{path}`")]
    InvalidColor { path: String, value: String },
    #[error("invalid theme value at `{path}`: expected a color string")]
    NotAColor { path: String },
    #[error("theme reference `{reference}` at `{path}` was not found")]
    UnresolvedRef { path: String, reference: String },
    #[error("circular theme reference at `{0}`")]
    RefCycle(String),
    #[error("circular hue alias at `{0}`")]
    HueCycle(String),
    #[error("required theme role `{path}` is missing from the asset")]
    MissingRole { path: &'static str },
}

/// Resolved role colors used by the views. Every field is validated against
/// the palette at load time, so accessors are infallible.
#[derive(Clone, Debug)]
struct Roles {
    text: Color,
    text_muted: Color,
    primary: Color,
    background: Color,
    background_raised: Color,
    background_raised_high: Color,
    background_raised_max: Color,
    border: Color,
    border_active: Color,
    scrollbar: Color,
    error: Color,
    warning: Color,
    success: Color,
    info: Color,
    diff_added: Color,
    diff_removed: Color,
    diff_context: Color,
    diff_hunk_header: Color,
    diff_added_background: Color,
    diff_removed_background: Color,
    diff_context_background: Color,
    diff_highlight_added: Color,
    diff_highlight_removed: Color,
    diff_line_number: Color,
    assistant_text: Color,
    syntax: [Color; SyntaxToken::ALL.len()],
    markdown: [Color; MarkdownToken::ALL.len()],
}

impl Roles {
    fn resolve(palette: &BTreeMap<String, Rgba>, underlay: Rgba) -> Result<Self, ThemeError> {
        let need = |path: &'static str| -> Result<Color, ThemeError> {
            palette
                .get(path)
                .map(|rgba| rgba.to_color(underlay))
                .ok_or(ThemeError::MissingRole { path })
        };
        let mut syntax = [Color::Reset; SyntaxToken::ALL.len()];
        for token in SyntaxToken::ALL {
            syntax[token.index()] = need(token.path())?;
        }
        let mut markdown = [Color::Reset; MarkdownToken::ALL.len()];
        for token in MarkdownToken::ALL {
            markdown[token.index()] = need(token.path())?;
        }
        Ok(Self {
            text: need("text.base")?,
            text_muted: need("text.muted")?,
            primary: need("hue.interactive.200")?,
            background: need("background.base")?,
            background_raised: need("background.raised.base")?,
            background_raised_high: need("background.raised.high")?,
            background_raised_max: need("background.raised.max")?,
            border: need("border.base")?,
            border_active: need("text.formfield.$focused")?,
            scrollbar: need("scrollbar.base")?,
            error: need("text.feedback.error.base")?,
            warning: need("text.feedback.warning.base")?,
            success: need("text.feedback.success.base")?,
            info: need("text.feedback.info.base")?,
            diff_added: need("diff.text.added")?,
            diff_removed: need("diff.text.removed")?,
            diff_context: need("diff.text.context")?,
            diff_hunk_header: need("diff.text.hunkHeader")?,
            diff_added_background: need("diff.background.added")?,
            diff_removed_background: need("diff.background.removed")?,
            diff_context_background: need("diff.background.context")?,
            diff_highlight_added: need("diff.highlight.added")?,
            diff_highlight_removed: need("diff.highlight.removed")?,
            diff_line_number: need("diff.lineNumber.text")?,
            assistant_text: need("markdown.text")?,
            syntax,
            markdown,
        })
    }
}

/// A resolved palette for one [`ThemeMode`].
#[derive(Clone, Debug)]
pub struct Theme {
    mode: ThemeMode,
    palette: BTreeMap<String, Rgba>,
    categorical: Vec<String>,
    underlay: Rgba,
    roles: Roles,
}

impl Theme {
    /// The upstream default theme: `opencode`, mode `dark`.
    ///
    /// Panics only if the vendored asset is malformed; the palette parity
    /// test keeps that impossible.
    pub fn dark() -> &'static Theme {
        static DARK: LazyLock<Theme> = LazyLock::new(|| vendored(ThemeMode::Dark));
        &DARK
    }

    /// The `light` mode of the same asset.
    pub fn light() -> &'static Theme {
        static LIGHT: LazyLock<Theme> = LazyLock::new(|| vendored(ThemeMode::Light));
        &LIGHT
    }

    /// Load the vendored asset in `mode`.
    pub fn load(mode: ThemeMode) -> Result<Theme, ThemeError> {
        Self::from_json(OPENCODE_THEME_JSON, mode)
    }

    /// Parse an asset document (same shape as the vendored v2 asset).
    pub fn from_json(json: &str, mode: ThemeMode) -> Result<Theme, ThemeError> {
        let root: Value = serde_json::from_str(json).map_err(ThemeError::Json)?;
        let root = root.as_object().ok_or(ThemeError::RootNotObject)?;
        let base = root.get("base").ok_or(ThemeError::MissingSection("base"))?;
        let overlay = root
            .get(mode.key())
            .ok_or(ThemeError::MissingSection(mode.key()))?;
        let merged = merge(base, overlay);
        let hue_definition = merged.get("hue").ok_or(ThemeError::MissingSection("hue"))?;
        let hues = resolve_hues(hue_definition)?;
        let mut palette = hues.clone();
        resolve_tokens(&merged, &hues, &mut palette)?;
        let categorical = merged
            .get("categorical")
            .and_then(Value::as_array)
            .ok_or(ThemeError::MissingSection("categorical"))?
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ThemeError::NotAColor {
                        path: format!("categorical.{index}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let underlay = palette
            .get("background.base")
            .copied()
            .unwrap_or(Rgba::BLACK)
            .over(Rgba::BLACK);
        let roles = Roles::resolve(&palette, underlay)?;
        Ok(Self {
            mode,
            palette,
            categorical,
            underlay,
            roles,
        })
    }

    /// Selected mode.
    pub fn mode(&self) -> ThemeMode {
        self.mode
    }

    /// Every resolved slot keyed by dotted asset path.
    pub fn palette(&self) -> &BTreeMap<String, Rgba> {
        &self.palette
    }

    /// Raw resolved value at `path`, alpha included.
    pub fn rgba(&self, path: &str) -> Option<Rgba> {
        self.palette.get(path).copied()
    }

    /// Opaque terminal color at `path` (alpha composited over the root
    /// background).
    pub fn color(&self, path: &str) -> Option<Color> {
        self.rgba(path).map(|rgba| rgba.to_color(self.underlay))
    }

    /// Categorical hue names in asset order.
    pub fn categorical(&self) -> &[String] {
        &self.categorical
    }

    // ---- role accessors (upstream names) --------------------------------

    /// Primary text, upstream `text.base`.
    pub fn text(&self) -> Color {
        self.roles.text
    }

    /// Muted text, upstream `text.muted`.
    pub fn text_muted(&self) -> Color {
        self.roles.text_muted
    }

    /// Primary accent, upstream v1 role `primary`; the v1 migration maps it
    /// to `$hue.interactive.200` (`packages/theme/src/tui/v1-migrate.ts`).
    pub fn primary(&self) -> Color {
        self.roles.primary
    }

    /// Root background, upstream `background.base` (the frame fills this).
    pub fn background(&self) -> Color {
        self.roles.background
    }

    /// Raised panel background, upstream `background.raised.base`; the v1
    /// role `backgroundPanel` migrates here.
    pub fn background_panel(&self) -> Color {
        self.roles.background_raised
    }

    /// Higher raised surface, upstream `background.raised.high`.
    pub fn background_raised_high(&self) -> Color {
        self.roles.background_raised_high
    }

    /// Highest raised surface, upstream `background.raised.max`.
    pub fn background_raised_max(&self) -> Color {
        self.roles.background_raised_max
    }

    /// Default border, upstream `border.base`.
    pub fn border(&self) -> Color {
        self.roles.border
    }

    /// Focus accent for active borders; v2 has no `borderActive` token, so
    /// this is the focus state `text.formfield.$focused`.
    pub fn border_active(&self) -> Color {
        self.roles.border_active
    }

    /// Scrollbar, upstream `scrollbar.base`.
    pub fn scrollbar(&self) -> Color {
        self.roles.scrollbar
    }

    /// Error feedback, upstream `text.feedback.error.base`.
    pub fn error(&self) -> Color {
        self.roles.error
    }

    /// Warning feedback, upstream `text.feedback.warning.base`.
    pub fn warning(&self) -> Color {
        self.roles.warning
    }

    /// Success feedback, upstream `text.feedback.success.base`.
    pub fn success(&self) -> Color {
        self.roles.success
    }

    /// Info feedback, upstream `text.feedback.info.base`.
    pub fn info(&self) -> Color {
        self.roles.info
    }

    /// Added diff text, upstream `diff.text.added`.
    pub fn diff_added(&self) -> Color {
        self.roles.diff_added
    }

    /// Removed diff text, upstream `diff.text.removed`.
    pub fn diff_removed(&self) -> Color {
        self.roles.diff_removed
    }

    /// Context diff text, upstream `diff.text.context`.
    pub fn diff_context(&self) -> Color {
        self.roles.diff_context
    }

    /// Diff hunk header, upstream `diff.text.hunkHeader`.
    pub fn diff_hunk_header(&self) -> Color {
        self.roles.diff_hunk_header
    }

    /// Added diff background, upstream `diff.background.added`.
    pub fn diff_added_background(&self) -> Color {
        self.roles.diff_added_background
    }

    /// Removed diff background, upstream `diff.background.removed`.
    pub fn diff_removed_background(&self) -> Color {
        self.roles.diff_removed_background
    }

    /// Context diff background, upstream `diff.background.context`.
    pub fn diff_context_background(&self) -> Color {
        self.roles.diff_context_background
    }

    /// Added diff highlight, upstream `diff.highlight.added`.
    pub fn diff_highlight_added(&self) -> Color {
        self.roles.diff_highlight_added
    }

    /// Removed diff highlight, upstream `diff.highlight.removed`.
    pub fn diff_highlight_removed(&self) -> Color {
        self.roles.diff_highlight_removed
    }

    /// Diff line number, upstream `diff.lineNumber.text`.
    pub fn diff_line_number(&self) -> Color {
        self.roles.diff_line_number
    }

    /// User message surface; upstream renders user rows on
    /// `background.raised.base` (`routes/session/index.tsx:2298-2345`).
    pub fn user_message_background(&self) -> Color {
        self.roles.background_raised
    }

    /// Assistant body text, upstream `markdown.text`
    /// (`routes/session/message-parts.tsx:156-171`).
    pub fn assistant_text(&self) -> Color {
        self.roles.assistant_text
    }

    /// Syntax token color, upstream `syntax.<token>`.
    pub fn syntax(&self, token: SyntaxToken) -> Color {
        self.roles.syntax[token.index()]
    }

    /// Markdown token color, upstream `markdown.<token>`.
    pub fn markdown(&self, token: MarkdownToken) -> Color {
        self.roles.markdown[token.index()]
    }
}

fn vendored(mode: ThemeMode) -> Theme {
    Theme::load(mode).unwrap_or_else(|error| panic!("vendored opencode theme is invalid: {error}"))
}

/// Deep merge: objects merge recursively, everything else is replaced by the
/// overlay (upstream `mergeTheme`).
fn merge(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            let mut merged = base.clone();
            for (key, value) in overlay {
                let item = match merged.get(key) {
                    Some(existing) => merge(existing, value),
                    None => value.clone(),
                };
                merged.insert(key.clone(), item);
            }
            Value::Object(merged)
        }
        _ => overlay.clone(),
    }
}

/// Resolve every hue scale into `hue.<name>.<step>` values, aliases included.
fn resolve_hues(definition: &Value) -> Result<BTreeMap<String, Rgba>, ThemeError> {
    let scales = definition
        .as_object()
        .ok_or(ThemeError::MissingSection("hue"))?;
    for name in scales.keys() {
        if !HUE_NAMES.contains(&name.as_str()) {
            return Err(ThemeError::UnknownHue(name.clone()));
        }
    }
    let mut cache: BTreeMap<String, BTreeMap<u16, Rgba>> = BTreeMap::new();
    for name in HUE_NAMES {
        hue_scale(scales, name, &mut cache, &mut Vec::new())?;
    }
    let mut resolved = BTreeMap::new();
    for name in HUE_NAMES {
        let scale = &cache[name];
        for step in HUE_STEPS {
            resolved.insert(format!("hue.{name}.{step}"), scale[&step]);
        }
    }
    Ok(resolved)
}

fn hue_scale(
    scales: &serde_json::Map<String, Value>,
    name: &str,
    cache: &mut BTreeMap<String, BTreeMap<u16, Rgba>>,
    stack: &mut Vec<String>,
) -> Result<BTreeMap<u16, Rgba>, ThemeError> {
    if let Some(cached) = cache.get(name) {
        return Ok(cached.clone());
    }
    if stack.iter().any(|entry| entry == name) {
        return Err(ThemeError::HueCycle(name.to_string()));
    }
    let value = scales
        .get(name)
        .ok_or_else(|| ThemeError::UnknownHue(name.to_string()))?;
    let scale = match value {
        Value::String(alias) => {
            let target = alias
                .strip_prefix("$hue.")
                .filter(|target| !target.is_empty() && !target.contains('.'))
                .ok_or_else(|| ThemeError::InvalidHueAlias {
                    path: format!("hue.{name}"),
                    value: alias.clone(),
                })?;
            stack.push(name.to_string());
            let resolved = hue_scale(scales, target, cache, stack);
            stack.pop();
            resolved?
        }
        Value::Object(steps) => {
            let mut scale = BTreeMap::new();
            for (key, item) in steps {
                let step: u16 = key
                    .parse()
                    .ok()
                    .filter(|step| HUE_STEPS.contains(step))
                    .ok_or_else(|| ThemeError::InvalidHueStep {
                        name: name.to_string(),
                        step: key.clone(),
                    })?;
                let color = item.as_str().ok_or_else(|| ThemeError::NotAColor {
                    path: format!("hue.{name}.{key}"),
                })?;
                let rgba = Rgba::from_hex(color).ok_or_else(|| ThemeError::InvalidColor {
                    path: format!("hue.{name}.{key}"),
                    value: color.to_string(),
                })?;
                scale.insert(step, rgba);
            }
            for step in HUE_STEPS {
                if !scale.contains_key(&step) {
                    return Err(ThemeError::MissingHueStep {
                        name: name.to_string(),
                        step,
                    });
                }
            }
            scale
        }
        other => {
            return Err(ThemeError::InvalidHueAlias {
                path: format!("hue.{name}"),
                value: other.to_string(),
            });
        }
    };
    cache.insert(name.to_string(), scale.clone());
    Ok(scale)
}

/// Walk the merged token tree (everything but `hue`/`categorical`) and store
/// each resolved color under its dotted path.
fn resolve_tokens(
    document: &Value,
    hues: &BTreeMap<String, Rgba>,
    palette: &mut BTreeMap<String, Rgba>,
) -> Result<(), ThemeError> {
    let root = document.as_object().ok_or(ThemeError::RootNotObject)?;
    for (key, value) in root {
        if key == "hue" || key == "categorical" {
            continue;
        }
        walk(value, key, document, hues, palette, &mut Vec::new())?;
    }
    Ok(())
}

fn walk(
    value: &Value,
    path: &str,
    document: &Value,
    hues: &BTreeMap<String, Rgba>,
    palette: &mut BTreeMap<String, Rgba>,
    stack: &mut Vec<String>,
) -> Result<(), ThemeError> {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                walk(
                    item,
                    &format!("{path}.{key}"),
                    document,
                    hues,
                    palette,
                    stack,
                )?;
            }
            Ok(())
        }
        Value::String(text) => {
            let rgba = resolve_string(text, path, document, hues, stack)?;
            palette.insert(path.to_string(), rgba);
            Ok(())
        }
        Value::Number(_) => Err(ThemeError::InvalidColor {
            path: path.to_string(),
            value: value.to_string(),
        }),
        _ => Err(ThemeError::NotAColor {
            path: path.to_string(),
        }),
    }
}

fn resolve_string(
    value: &str,
    path: &str,
    document: &Value,
    hues: &BTreeMap<String, Rgba>,
    stack: &mut Vec<String>,
) -> Result<Rgba, ThemeError> {
    if value == "transparent" {
        return Ok(Rgba::TRANSPARENT);
    }
    if value.starts_with('#') {
        return Rgba::from_hex(value).ok_or_else(|| ThemeError::InvalidColor {
            path: path.to_string(),
            value: value.to_string(),
        });
    }
    if let Some(reference) = value.strip_prefix('$') {
        return resolve_reference(reference, path, document, hues, stack);
    }
    Err(ThemeError::InvalidColor {
        path: path.to_string(),
        value: value.to_string(),
    })
}

fn resolve_reference(
    reference: &str,
    path: &str,
    document: &Value,
    hues: &BTreeMap<String, Rgba>,
    stack: &mut Vec<String>,
) -> Result<Rgba, ThemeError> {
    if reference.starts_with("hue.") {
        return hues
            .get(reference)
            .copied()
            .ok_or_else(|| ThemeError::UnresolvedRef {
                path: path.to_string(),
                reference: reference.to_string(),
            });
    }
    if stack.iter().any(|entry| entry == reference) {
        return Err(ThemeError::RefCycle(reference.to_string()));
    }
    let target = read_path(document, reference).ok_or_else(|| ThemeError::UnresolvedRef {
        path: path.to_string(),
        reference: reference.to_string(),
    })?;
    let target = target.as_str().ok_or_else(|| ThemeError::NotAColor {
        path: reference.to_string(),
    })?;
    stack.push(reference.to_string());
    let resolved = resolve_string(target, reference, document, hues, stack);
    stack.pop();
    resolved
}

fn read_path<'a>(document: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = document;
    for segment in path.split('.') {
        current = current.as_object()?.get(segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Token slots (non-hue, non-categorical) in the merged v2 asset.
    const TOKEN_SLOTS: usize = 74;
    /// 11 hue names x 9 steps.
    const HUE_SLOTS: usize = 99;

    fn load(mode: ThemeMode) -> Theme {
        Theme::from_json(OPENCODE_THEME_JSON, mode).expect("vendored asset must load")
    }

    /// Test-local merge (independent copy of the production semantics).
    fn merge_reference(base: &Value, overlay: &Value) -> Value {
        match (base, overlay) {
            (Value::Object(base), Value::Object(overlay)) => {
                let mut merged = base.clone();
                for (key, value) in overlay {
                    let item = match merged.get(key) {
                        Some(existing) => merge_reference(existing, value),
                        None => value.clone(),
                    };
                    merged.insert(key.clone(), item);
                }
                Value::Object(merged)
            }
            _ => overlay.clone(),
        }
    }

    /// Test-local raw JSON resolver (independent walk of the asset).
    fn raw_resolve(document: &Value, path: &str) -> Rgba {
        if let Some(rest) = path.strip_prefix("hue.") {
            let (name, step) = rest.split_once('.').expect("hue path");
            let scale = raw_hue_scale(document, name).expect("hue scale");
            let value = scale
                .get(step)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("hue step {path}"));
            return Rgba::from_hex(value).unwrap_or_else(|| panic!("bad hue hex at {path}"));
        }
        let value = read_path(document, path).unwrap_or_else(|| panic!("missing raw path {path}"));
        let text = value
            .as_str()
            .unwrap_or_else(|| panic!("not a color at {path}: {value}"));
        match_text(document, path, text)
    }

    fn match_text(document: &Value, path: &str, text: &str) -> Rgba {
        if text == "transparent" {
            return Rgba::TRANSPARENT;
        }
        if let Some(reference) = text.strip_prefix('$') {
            return raw_resolve(document, reference);
        }
        Rgba::from_hex(text).unwrap_or_else(|| panic!("bad hex `{text}` at {path}"))
    }

    fn raw_hue_scale<'a>(
        document: &'a Value,
        name: &str,
    ) -> Option<&'a serde_json::Map<String, Value>> {
        let hues = document.get("hue")?.as_object()?;
        let mut current = hues.get(name)?;
        let mut hops = 0;
        while let Value::String(alias) = current {
            hops += 1;
            assert!(hops < 16, "hue alias loop at {name}");
            current = hues.get(alias.strip_prefix("$hue.")?)?;
        }
        current.as_object()
    }

    fn assert_leaves(
        document: &Value,
        value: &Value,
        path: &str,
        theme: &Theme,
        checked: &mut usize,
    ) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    assert_leaves(document, item, &format!("{path}.{key}"), theme, checked);
                }
            }
            Value::String(_) => {
                let expected = raw_resolve(document, path);
                let actual = theme
                    .rgba(path)
                    .unwrap_or_else(|| panic!("palette dropped slot {path}"));
                assert_eq!(
                    actual, expected,
                    "slot {path} differs from the vendored asset"
                );
                *checked += 1;
            }
            other => panic!("unexpected non-color leaf at {path}: {other}"),
        }
    }

    /// Every leaf slot and hue value must equal the raw vendored JSON; the
    /// walk is generic, so a dropped slot cannot hide behind a pass.
    #[test]
    fn palette_matches_vendored_asset() {
        let raw: Value = serde_json::from_str(OPENCODE_THEME_JSON).expect("vendored JSON");
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let theme = load(mode);
            let merged = merge_reference(&raw["base"], &raw[mode.key()]);

            let mut checked = 0;
            for (key, value) in merged.as_object().expect("merged object") {
                if key == "hue" || key == "categorical" {
                    continue;
                }
                assert_leaves(&merged, value, key, &theme, &mut checked);
            }
            assert_eq!(checked, TOKEN_SLOTS, "{mode:?}: token slot count changed");

            let mut hue_checked = 0;
            for name in HUE_NAMES {
                for step in HUE_STEPS {
                    let path = format!("hue.{name}.{step}");
                    let expected = raw_resolve(&merged, &path);
                    let actual = theme
                        .rgba(&path)
                        .unwrap_or_else(|| panic!("palette dropped {path}"));
                    assert_eq!(actual, expected, "{path} differs from the asset");
                    hue_checked += 1;
                }
            }
            assert_eq!(hue_checked, HUE_SLOTS, "{mode:?}: hue slot count changed");

            let categorical: Vec<String> = merged["categorical"]
                .as_array()
                .expect("categorical")
                .iter()
                .map(|item| item.as_str().expect("hue name").to_string())
                .collect();
            assert_eq!(theme.categorical(), categorical.as_slice());

            assert_eq!(
                theme.palette().len(),
                TOKEN_SLOTS + HUE_SLOTS,
                "{mode:?}: palette size changed"
            );
        }
    }

    #[test]
    fn refs_and_hue_aliases_resolve_like_upstream() {
        let dark = Theme::dark();
        assert_eq!(
            dark.rgba("text.base"),
            Some(Rgba::new(0xee, 0xee, 0xee, 255))
        );
        assert_eq!(dark.rgba("hue.yellow.200"), dark.rgba("hue.gray.200"));
        assert_eq!(dark.rgba("hue.accent.200"), dark.rgba("hue.purple.200"));
        assert_eq!(
            dark.rgba("hue.interactive.200"),
            dark.rgba("hue.orange.200")
        );
        assert_eq!(dark.rgba("hue.neutral.700"), dark.rgba("hue.gray.700"));
        assert_eq!(
            dark.rgba("background.raised.base"),
            dark.rgba("hue.neutral.700")
        );
        assert_eq!(
            dark.rgba("text.formfield.$focused"),
            dark.rgba("hue.interactive.200")
        );
        assert_eq!(
            dark.rgba("background.action.primary.base"),
            Some(Rgba::TRANSPARENT)
        );
        assert_eq!(dark.background(), Color::Rgb(0x0a, 0x0a, 0x0a));
        assert_eq!(dark.border(), Color::Rgb(0x48, 0x48, 0x48));
        assert_eq!(dark.primary(), Color::Rgb(0xfa, 0xb2, 0x83));
        assert_eq!(dark.error(), Color::Rgb(0xe0, 0x6c, 0x75));

        let light = Theme::light();
        assert_eq!(light.rgba("hue.accent.200"), light.rgba("hue.orange.200"));
        assert_eq!(
            light.rgba("hue.interactive.200"),
            light.rgba("hue.blue.200")
        );
        assert_eq!(light.background(), Color::Rgb(0xff, 0xff, 0xff));
        assert_eq!(light.border(), Color::Rgb(0xb8, 0xb8, 0xb8));
    }

    #[test]
    fn alpha_is_composited_over_the_root_background() {
        let theme = Theme::dark();
        let underlay = theme.rgba("background.base").expect("root background");
        // Opaque values map straight to truecolor.
        assert_eq!(
            Rgba::new(0x48, 0x48, 0x48, 255).to_color(underlay),
            Color::Rgb(0x48, 0x48, 0x48)
        );
        // Straight alpha source-over: white at 50% over black is mid gray.
        assert_eq!(
            Rgba::new(255, 255, 255, 128).to_color(Rgba::BLACK),
            Color::Rgb(128, 128, 128)
        );
        // Fully transparent falls back to the underlay.
        assert_eq!(
            Rgba::TRANSPARENT.to_color(Rgba::new(10, 20, 30, 255)),
            Color::Rgb(10, 20, 30)
        );
        // Hex with alpha has an exact byte form.
        assert_eq!(
            Rgba::from_hex("#80402040"),
            Some(Rgba::new(0x80, 0x40, 0x20, 0x40))
        );
        // A synthetic alpha slot resolves through the same pipeline:
        // #80402040 over dark background.base #0a0a0a.
        let mut root: Value = serde_json::from_str(OPENCODE_THEME_JSON).expect("vendored JSON");
        root["dark"]["border"]["base"] = json!("#80402040");
        let alpha = Theme::from_json(&root.to_string(), ThemeMode::Dark).expect("alpha asset");
        assert_eq!(alpha.border(), Color::Rgb(39, 23, 15));
    }

    #[test]
    fn malformed_assets_fail_loudly() {
        assert!(matches!(
            Theme::from_json("{", ThemeMode::Dark),
            Err(ThemeError::Json(_))
        ));
        assert!(matches!(
            Theme::from_json("[]", ThemeMode::Dark),
            Err(ThemeError::RootNotObject)
        ));

        let base =
            || -> Value { serde_json::from_str(OPENCODE_THEME_JSON).expect("vendored JSON") };

        let mut root = base();
        root.as_object_mut().expect("root").remove("dark");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::MissingSection("dark"))
        ));

        let mut root = base();
        root["dark"]["border"]["base"] = json!("#zzzzzz");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::InvalidColor { .. })
        ));

        let mut root = base();
        root["dark"]["border"]["base"] = json!(42);
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::InvalidColor { .. })
        ));

        let mut root = base();
        root["dark"]["border"]["base"] = json!("slate");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::InvalidColor { .. })
        ));

        let mut root = base();
        root["dark"]["border"]["base"] = json!("$not.there");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::UnresolvedRef { .. })
        ));

        let mut root = base();
        root["dark"]["border"]["base"] = json!("$scrollbar.base");
        root["dark"]["scrollbar"]["base"] = json!("$border.base");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::RefCycle(_))
        ));

        let mut root = base();
        root["base"].as_object_mut().expect("base").remove("border");
        root["dark"].as_object_mut().expect("dark").remove("border");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::MissingRole {
                path: "border.base"
            })
        ));

        let mut root = base();
        root["dark"]["hue"]["teal"] = json!({ "100": "#000000" });
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::UnknownHue(name)) if name == "teal"
        ));

        let mut root = base();
        root["dark"]["hue"]["accent"] = json!("$hue.interactive");
        root["dark"]["hue"]["interactive"] = json!("$hue.accent");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::HueCycle(_))
        ));

        let mut root = base();
        root["dark"]["hue"]["gray"] = json!({ "100": "#000000" });
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::MissingHueStep { .. })
        ));

        let mut root = base();
        root["dark"]["hue"]["gray"] = json!("$text.base");
        assert!(matches!(
            Theme::from_json(&root.to_string(), ThemeMode::Dark),
            Err(ThemeError::InvalidHueAlias { .. })
        ));
    }

    #[test]
    fn every_role_accessor_is_resolved() {
        let dark = Theme::dark();
        for token in SyntaxToken::ALL {
            assert!(
                !matches!(dark.syntax(token), Color::Reset),
                "syntax token {token:?} must come from the asset"
            );
        }
        for token in MarkdownToken::ALL {
            assert!(
                !matches!(dark.markdown(token), Color::Reset),
                "markdown token {token:?} must come from the asset"
            );
        }
        assert_eq!(dark.text(), Color::Rgb(0xee, 0xee, 0xee));
        assert_eq!(dark.text_muted(), Color::Rgb(0x80, 0x80, 0x80));
        assert_eq!(dark.background_panel(), Color::Rgb(0x14, 0x14, 0x14));
        assert_eq!(dark.scrollbar(), Color::Rgb(0x60, 0x60, 0x60));
        assert_eq!(dark.warning(), Color::Rgb(0xf5, 0xa7, 0x42));
        assert_eq!(dark.success(), Color::Rgb(0x7f, 0xd8, 0x8f));
        assert_eq!(dark.info(), Color::Rgb(0x56, 0xb6, 0xc2));
        assert_eq!(dark.diff_added(), Color::Rgb(0x4f, 0xd6, 0xbe));
        assert_eq!(dark.diff_removed(), Color::Rgb(0xc5, 0x3b, 0x53));
        assert_eq!(dark.diff_context(), Color::Rgb(0x82, 0x8b, 0xb8));
    }
}
