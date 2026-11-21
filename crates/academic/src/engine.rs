//! The TeX engine a template is typeset with.
//!
//! pdfLaTeX sets Western European text with Latin Modern. XeLaTeX reads OpenType fonts, which the
//! templates use for Chinese and Japanese (xeCJK with Noto Serif and Sans CJK). LuaLaTeX is not
//! offered: its Lua `io` and `os` libraries can read and write files outside kpathsea's
//! `openin_any`/`openout_any` checks, so a document could escape the project sandbox.

use serde::{Deserialize, Serialize};

/// A TeX engine run through latexmk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Pdflatex,
    Xelatex,
}

impl Engine {
    /// The name used in `template.toml` and the project file.
    pub fn name(self) -> &'static str {
        match self {
            Engine::Pdflatex => "pdflatex",
            Engine::Xelatex => "xelatex",
        }
    }

    /// The name people know it by, for messages: `pdfLaTeX`, `XeLaTeX`.
    pub fn title(self) -> &'static str {
        match self {
            Engine::Pdflatex => "pdfLaTeX",
            Engine::Xelatex => "XeLaTeX",
        }
    }

    /// The engine `name` names, if it is one PSG Academic PDF runs.
    pub fn parse(name: &str) -> Option<Engine> {
        match name {
            "pdflatex" => Some(Engine::Pdflatex),
            "xelatex" => Some(Engine::Xelatex),
            _ => None,
        }
    }

    /// The program latexmk runs.
    pub fn program(self) -> &'static str {
        self.name()
    }

    /// latexmk's switch selecting the engine, and its command template for it (with shell escape
    /// off; `%O` and `%S` are latexmk's placeholders). XeLaTeX writes `.xdv`, which latexmk turns
    /// into the PDF with xdvipdfmx.
    pub fn latexmk_args(self) -> [&'static str; 2] {
        match self {
            Engine::Pdflatex => ["-pdf", "-pdflatex=pdflatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error %O %S"],
            Engine::Xelatex => {
                ["-xelatex", "-xelatex=xelatex -no-pdf -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error %O %S"]
            }
        }
    }

    /// Whether it can set text in `script` with the fonts PSG Academic PDF provides.
    pub fn sets(self, script: crate::scripts::Script) -> bool {
        use crate::scripts::Script;
        self == Engine::Xelatex && matches!(script, Script::Han | Script::Kana)
    }
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_lualatex_is_refused() {
        for e in [Engine::Pdflatex, Engine::Xelatex] {
            assert_eq!(Engine::parse(e.name()), Some(e));
        }
        assert_eq!(Engine::parse("lualatex"), None);
    }

    #[test]
    fn every_engine_runs_without_shell_escape() {
        for e in [Engine::Pdflatex, Engine::Xelatex] {
            let [switch, template] = e.latexmk_args();
            assert!(switch.starts_with('-'));
            assert!(template.contains("-no-shell-escape"), "{template}");
            assert!(template.starts_with(&format!("-{}={}", e.name(), e.program())), "{template}");
        }
    }
}
