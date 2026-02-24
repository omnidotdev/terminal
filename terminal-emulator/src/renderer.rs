use crate::grid::TerminalGrid;
use sugarloaf::{FragmentStyle, FragmentStyleDecoration, Sugarloaf, UnderlineInfo, UnderlineShape};

/// Render the terminal grid into sugarloaf content
pub fn render_grid(
    sugarloaf: &mut Sugarloaf,
    grid: &TerminalGrid,
    rt_id: usize,
) {
    // Clone the font library (Arc-shared) for per-character font matching.
    // This enables Nerd Font glyphs to render on Android by finding the
    // correct fallback font for non-ASCII characters.
    let font_library = sugarloaf.content().font_library().clone();
    let content = sugarloaf.content();
    content.sel(rt_id).clear();

    // Hold a read lock for font lookups; must be dropped before build()
    // which acquires a write lock for font metrics
    {
        let font_lib = font_library.inner.read();

        for row_idx in 0..grid.rows {
            let row = grid.visible_row(row_idx);
            // Scrollback rows may have a different column count after resize
            let cols = grid.cols.min(row.len());
            let mut run_start = 0;

            while run_start < cols {
                let cell = &row[run_start];

                // Build a style for the current cell
                let (fg, bg) = if cell.inverse {
                    (
                        cell.bg.unwrap_or([0.05, 0.05, 0.1, 1.0]),
                        Some(cell.fg),
                    )
                } else {
                    (cell.fg, cell.bg)
                };

                // Selection highlight: swap fg/bg
                let (fg, bg) = if grid.is_selected(run_start, row_idx) {
                    (
                        bg.unwrap_or([0.05, 0.05, 0.1, 1.0]),
                        Some(fg),
                    )
                } else {
                    (fg, bg)
                };

                let decoration = if cell.underline {
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }))
                } else {
                    None
                };

                let style = FragmentStyle {
                    color: fg,
                    background_color: bg,
                    decoration,
                    ..FragmentStyle::default()
                };

                // Batch consecutive characters with the same visual style
                let mut run_end = run_start + 1;
                while run_end < cols {
                    let next = &row[run_end];
                    let (nfg, nbg) = if next.inverse {
                        (
                            next.bg.unwrap_or([0.05, 0.05, 0.1, 1.0]),
                            Some(next.fg),
                        )
                    } else {
                        (next.fg, next.bg)
                    };
                    if nfg == fg
                        && nbg == bg
                        && next.bold == cell.bold
                        && next.italic == cell.italic
                        && next.underline == cell.underline
                        && grid.is_selected(run_end, row_idx)
                            == grid.is_selected(run_start, row_idx)
                    {
                        run_end += 1;
                    } else {
                        break;
                    }
                }

                // Sub-split by font_id so non-ASCII glyphs (Nerd Font icons,
                // emoji, CJK) resolve to the correct fallback font
                let mut sub_start = run_start;
                while sub_start < run_end {
                    let ch = row[sub_start].c;
                    let (font_id, is_emoji) = if ch.is_ascii() {
                        (0, false)
                    } else {
                        font_lib
                            .find_best_font_match(ch, &style)
                            .unwrap_or((0, false))
                    };

                    // Extend sub-run while consecutive chars share the same font
                    let mut sub_end = sub_start + 1;
                    while sub_end < run_end {
                        let next_ch = row[sub_end].c;
                        let next_font_id = if next_ch.is_ascii() {
                            0
                        } else {
                            font_lib
                                .find_best_font_match(next_ch, &style)
                                .map_or(0, |(id, _)| id)
                        };
                        if next_font_id == font_id {
                            sub_end += 1;
                        } else {
                            break;
                        }
                    }

                    let text: String =
                        row[sub_start..sub_end].iter().map(|c| c.c).collect();

                    let mut sub_style = style;
                    sub_style.font_id = font_id;
                    if is_emoji {
                        sub_style.width = 2.0;
                    }

                    content.add_text(&text, sub_style);
                    sub_start = sub_end;
                }

                run_start = run_end;
            }

            // Cursor only visible when viewing live output
            if grid.display_offset == 0
                && row_idx == grid.cursor_row
                && grid.cursor_col < grid.cols
            {
                // Cursor is rendered as part of the content — the cursor block
                // is already included in the text above via the cell character
            }

            content.new_line();
        }
    }

    content.build();
}
