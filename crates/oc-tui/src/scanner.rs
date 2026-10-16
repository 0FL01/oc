//! Pinned v2.0.12 `ui/spinner.ts:25-190,199-243,272-368` block scanner.
//! A terminal has no alpha channel: blend the source RGBA against the footer's
//! painted `background.base`, rather than the raised prompt input surface.
//! Keep scanner color math local: the theme's integer `Rgba::over` truncates.

use ratatui::{
    style::{Color, Style},
    text::Span,
};

pub(crate) const FRAMES: usize = 54; // 8 forward + 9 end + 7 back + 30 start
pub(crate) const FRAME_MS: u64 = 40;
const WIDTH: usize = 8;
const TRAIL: usize = 6;

pub(crate) fn spans(frame: usize, agent: Color, background: Color) -> Vec<Span<'static>> {
    let (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) = (agent, background) else {
        return Vec::new();
    };
    let frame = frame % FRAMES;
    let (position, forward, holding, progress, total) = match frame {
        0..8 => (frame, true, false, frame, WIDTH),
        8..17 => (7, true, true, frame - 8, 9),
        17..24 => (6 - (frame - 17), false, false, frame - 17, WIDTH - 1),
        _ => (0, false, true, frame - 24, 30),
    };
    let fade = if holding {
        (1.0 - progress as f64 / total as f64 * 0.7).max(0.3)
    } else {
        0.3 + progress as f64 / (total - 1) as f64 * 0.7
    };
    (0..WIDTH)
        .map(|cell| {
            let distance = if forward {
                position as isize - cell as isize
            } else {
                cell as isize - position as isize
            };
            let index = if holding {
                distance + progress as isize
            } else if (0..TRAIL as isize).contains(&distance) {
                distance
            } else {
                -1
            };
            let (symbol, color, alpha) = if (0..TRAIL as isize).contains(&index) {
                let index = index as usize;
                let (bright, alpha) = match index {
                    0 => (1.0, 1.0),
                    1 => (1.15, 0.9),
                    _ => (1.0, 0.65_f64.powi(index as i32 - 1)),
                };
                let bloom = |value: u8| (f64::from(value) * bright).min(255.0).round() as u8;
                ("■", (bloom(r), bloom(g), bloom(b)), alpha)
            } else {
                ("⬝", (r, g, b), 0.6 * fade)
            };
            // Source RGBA channels and alpha are rounded to 8-bit at the
            // renderer boundary; round the final opaque blend only once.
            let alpha = (alpha * 255.0).round() as u8;
            let blend = |source: u8, under: u8| -> u8 {
                ((u32::from(source) * u32::from(alpha)
                    + u32::from(under) * u32::from(255 - alpha)
                    + 127)
                    / 255) as u8
            };
            Span::styled(
                symbol,
                Style::default().fg(Color::Rgb(
                    blend(color.0, br),
                    blend(color.1, bg),
                    blend(color.2, bb),
                )),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_sequence_and_directional_trail() {
        let draw = |frame| {
            spans(frame, Color::Rgb(100, 120, 140), Color::Rgb(10, 10, 10))
                .into_iter()
                .map(|span| span.content.to_string())
                .collect::<String>()
        };
        assert_eq!(draw(0), "■⬝⬝⬝⬝⬝⬝⬝");
        assert_eq!(draw(7), "⬝⬝■■■■■■");
        assert_eq!(draw(8), draw(7));
        assert_eq!(draw(13), "⬝⬝⬝⬝⬝⬝⬝■");
        assert_eq!(draw(14), "⬝⬝⬝⬝⬝⬝⬝⬝");
        assert_eq!(draw(17), "⬝⬝⬝⬝⬝⬝■■");
        assert_eq!(draw(23), "■■■■■■⬝⬝");
        assert_eq!(draw(24), draw(23));
        assert_eq!(draw(29), "■⬝⬝⬝⬝⬝⬝⬝");
        assert_eq!(draw(30), "⬝⬝⬝⬝⬝⬝⬝⬝");
        assert_eq!(draw(53), "⬝⬝⬝⬝⬝⬝⬝⬝");
        assert_eq!(draw(54), draw(0));
        for frame in 0..FRAMES {
            assert_eq!(draw(frame).chars().count(), WIDTH);
        }
    }

    #[test]
    fn bloom_trail_and_inactive_fade_composite_over_footer() {
        let bg = Color::Rgb(10, 10, 10);
        let bright = Color::Rgb(92, 156, 245); // pinned agent #5c9cf5
        let colors = |frame| {
            spans(frame, bright, bg)
                .into_iter()
                .map(|span| span.style.fg.unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(colors(0)[0], bright);
        assert_eq!(colors(0)[1], Color::Rgb(25, 36, 52)); // #192434
        assert_eq!(colors(1)[0], Color::Rgb(97, 162, 231)); // bloom #61a2e7
        assert_eq!(colors(1)[1], bright);
        assert_eq!(colors(1)[2], Color::Rgb(30, 45, 66)); // #1e2d42
        assert_eq!(colors(2)[0], Color::Rgb(63, 105, 163)); // trail #3f69a3
        assert_eq!(colors(3)[0], Color::Rgb(45, 72, 110)); // trail #2d486e
        assert_eq!(colors(4)[0], Color::Rgb(33, 50, 75)); // trail #21324b
        assert_eq!(colors(5)[0], Color::Rgb(25, 36, 52)); // trail #192434
        let tinted_bg = spans(1, bright, Color::Rgb(10, 20, 30));
        assert_eq!(tinted_bg[0].style.fg, Some(Color::Rgb(97, 163, 233)));
        assert_ne!(
            spans(8, bright, bg)[0].style.fg,
            spans(16, bright, bg)[0].style.fg
        );
        assert_ne!(
            spans(17, bright, bg)[0].style.fg,
            spans(23, bright, bg)[7].style.fg
        );
    }
}
