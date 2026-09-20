use crate::grid::{Cell, TerminalGrid};
use sugarloaf::{
    FragmentStyle, FragmentStyleDecoration, Sugarloaf, UnderlineInfo, UnderlineShape,
};

/// Default background color used when a cell has no explicit background
const DEFAULT_BG: [f32; 4] = [0.05, 0.05, 0.1, 1.0];

/// Compute effective fg/bg for a cell, accounting for inverse, selection, and cursor
fn cell_colors(
    cell: &Cell,
    is_selected: bool,
    is_cursor: bool,
) -> ([f32; 4], Option<[f32; 4]>) {
    // Cell inverse attribute
    let (mut fg, mut bg) = if cell.inverse {
        (cell.bg.unwrap_or(DEFAULT_BG), Some(cell.fg))
    } else {
        (cell.fg, cell.bg)
    };

    // Selection highlight: swap fg/bg
    if is_selected {
        let tmp = bg.unwrap_or(DEFAULT_BG);
        bg = Some(fg);
        fg = tmp;
    }

    // Cursor: swap fg/bg for block cursor
    if is_cursor {
        let tmp = bg.unwrap_or(DEFAULT_BG);
        bg = Some(fg);
        fg = tmp;
    }

    (fg, bg)
}

/// Render the terminal grid into sugarloaf content
pub fn render_grid(sugarloaf: &mut Sugarloaf, grid: &TerminalGrid, rt_id: usize) {
    // Clone the font library (Arc-shared) for per-character font matching.
    // This enables Nerd Font glyphs to render on Android by finding the
    // correct fallback font for non-ASCII characters.
    let font_library = sugarloaf.content().font_library().clone();
    let content = sugarloaf.content();
    content.sel(rt_id).clear();

    // Cursor is only visible when viewing live output
    let cursor_row = if grid.display_offset == 0 {
        Some(grid.cursor_row)
    } else {
        None
    };

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
                let is_cursor =
                    cursor_row == Some(row_idx) && run_start == grid.cursor_col;
                let is_selected = grid.is_selected(run_start, row_idx);

                let (fg, bg) = cell_colors(cell, is_selected, is_cursor);

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
                    let next_is_cursor =
                        cursor_row == Some(row_idx) && run_end == grid.cursor_col;
                    let next_is_selected = grid.is_selected(run_end, row_idx);
                    let (nfg, nbg) = cell_colors(next, next_is_selected, next_is_cursor);

                    if nfg == fg
                        && nbg == bg
                        && next.bold == cell.bold
                        && next.italic == cell.italic
                        && next.underline == cell.underline
                    {
                        run_end += 1;
                    } else {
                        break;
                    }
                }

                // Sub-split by font_id (so Nerd Font icons, emoji and CJK pick
                // the right fallback font) and by display width, skipping the
                // width-0 spacer cells that trail wide chars so they are not
                // drawn as extra glyphs
                let mut sub = run_start;
                while sub < run_end {
                    if row[sub].width == 0 {
                        sub += 1;
                        continue;
                    }
                    let ch = row[sub].c;
                    let wide = row[sub].width == 2;
                    let font_id = if ch.is_ascii() {
                        0
                    } else {
                        font_lib
                            .find_best_font_match(ch, &style)
                            .map_or(0, |(id, _)| id)
                    };

                    let mut text = String::new();
                    text.push(ch);

                    // Extend the run over cells that share the font and width,
                    // stepping over spacers without emitting them
                    let mut next = sub + 1;
                    while next < run_end {
                        if row[next].width == 0 {
                            next += 1;
                            continue;
                        }
                        let next_ch = row[next].c;
                        let next_wide = row[next].width == 2;
                        let next_font_id = if next_ch.is_ascii() {
                            0
                        } else {
                            font_lib
                                .find_best_font_match(next_ch, &style)
                                .map_or(0, |(id, _)| id)
                        };
                        if next_font_id == font_id && next_wide == wide {
                            text.push(next_ch);
                            next += 1;
                        } else {
                            break;
                        }
                    }

                    let mut sub_style = style;
                    sub_style.font_id = font_id;
                    if wide {
                        sub_style.width = 2.0;
                    }

                    content.add_text(&text, sub_style);
                    sub = next;
                }

                run_start = run_end;
            }

            content.new_line();
        }
    }

    content.build();
}
