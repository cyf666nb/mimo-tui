#![allow(dead_code)]
/// Syntax highlighting for code blocks in the TUI.
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    ps: SyntaxSet,
    ts: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            ps: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    }

    /// Highlight a code block and return styled Lines.
    /// `lang` is the language hint (e.g., "python", "rust", "json").
    pub fn highlight(&self, code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
        let syntax = if let Some(lang) = lang {
            self.ps.find_syntax_by_token(lang)
                .unwrap_or_else(|| self.ps.find_syntax_plain_text())
        } else {
            self.ps.find_syntax_by_first_line(code)
                .unwrap_or_else(|| self.ps.find_syntax_plain_text())
        };

        let theme = &self.ts.themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges = h.highlight_line(line, &self.ps).unwrap_or_default();
            let spans: Vec<Span> = ranges.iter().map(|(style, text)| {
                let fg = style.foreground;
                let color = Color::Rgb(fg.r, fg.g, fg.b);
                Span::styled(text.to_string(), Style::default().fg(color))
            }).collect();
            lines.push(Line::from(spans));
        }
        lines
    }

    /// Detect language from code fence (```python → "python")
    pub fn parse_fence_lang(fence: &str) -> Option<&str> {
        let lang = fence.trim_start_matches('`').trim();
        if lang.is_empty() {
            None
        } else {
            Some(lang)
        }
    }
}

/// Parse markdown text into structured blocks for rendering.
pub fn parse_markdown(text: &str) -> Vec<MdBlock> {
    let mut blocks = Vec::new();
    let mut in_code = false;
    let mut code_lang: Option<String> = None;
    let mut code_lines = Vec::new();

    for line in text.lines() {
        if line.starts_with("```") && !in_code {
            in_code = true;
            let lang = line.trim_start_matches('`').trim();
            code_lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
            code_lines.clear();
        } else if line.starts_with("```") && in_code {
            in_code = false;
            blocks.push(MdBlock::Code {
                lang: code_lang.clone(),
                code: code_lines.join("\n"),
            });
            code_lang = None;
            code_lines.clear();
        } else if in_code {
            code_lines.push(line.to_string());
        } else if let Some(rest) = line.strip_prefix("# ") {
            blocks.push(MdBlock::Heading(rest.to_string(), 1));
        } else if let Some(rest) = line.strip_prefix("## ") {
            blocks.push(MdBlock::Heading(rest.to_string(), 2));
        } else if let Some(rest) = line.strip_prefix("### ") {
            blocks.push(MdBlock::Heading(rest.to_string(), 3));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            blocks.push(MdBlock::Bullet(line[2..].to_string()));
        } else if line.starts_with("| ") && line.contains(" | ") {
            blocks.push(MdBlock::TableRow(line.to_string()));
        } else if line.trim().is_empty() {
            blocks.push(MdBlock::Empty);
        } else {
            blocks.push(MdBlock::Paragraph(line.to_string()));
        }
    }

    // Unclosed code block
    if in_code && !code_lines.is_empty() {
        blocks.push(MdBlock::Code {
            lang: code_lang,
            code: code_lines.join("\n"),
        });
    }

    blocks
}

#[derive(Debug)]
pub enum MdBlock {
    Heading(String, u8),
    Paragraph(String),
    Bullet(String),
    Code { lang: Option<String>, code: String },
    TableRow(String),
    Empty,
}
