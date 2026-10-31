//! Font metrics for measuring text in Lingo (`charPosToLoc`, a text
//! member's height). The page or a native host supplies real fonts through
//! [`director::text::Fonts`]; until then [`ApproxFonts`] uses Arial-like
//! proportions (⚠️ approximate: line breaks may differ from Director's).

use director::text::{Fonts, Glyph, LineMetrics};
use rifx::text::TextStyle;

pub struct ApproxFonts;

impl Fonts for ApproxFonts {
    fn line(&self, style: &TextStyle) -> LineMetrics {
        let s = style.size.max(1) as f32;
        LineMetrics {
            ascent: (s * 0.905).round(),
            descent: (s * 0.212).round(),
            gap: 0.0,
        }
    }

    fn glyph(&self, style: &TextStyle, ch: char) -> Glyph {
        let s = style.size.max(1) as f32;
        let w = match ch {
            ' ' | 'i' | 'l' | 'j' | '.' | ',' | ';' | ':' | '\'' | '!' | '|' => 0.28,
            'm' | 'w' | 'M' | 'W' => 0.83,
            c if c.is_uppercase() => 0.67,
            _ => 0.55,
        };
        Glyph {
            advance: (s * w * if style.bold { 1.07 } else { 1.0 }).round(),
            ..Default::default()
        }
    }
}
