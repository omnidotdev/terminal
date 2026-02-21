use crate::terminal::TerminalGrid;
use sugarloaf::{FragmentStyle, FragmentStyleDecoration, Sugarloaf, UnderlineInfo, UnderlineShape};

/// Render the terminal grid into sugarloaf content
pub fn render_grid(
    sugarloaf: &mut Sugarloaf,
    grid: &TerminalGrid,
    rt_id: usize,
) {
    let content = sugarloaf.content();
    content.sel(rt_id).clear();

    for row_idx in 0..grid.rows {
        let row = &grid.cells[row_idx];
        let mut run_start = 0;

        while run_start < grid.cols {
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

            // Batch consecutive characters with the same style
            let mut run_end = run_start + 1;
            while run_end < grid.cols {
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
                {
                    run_end += 1;
                } else {
                    break;
                }
            }

            // Collect the text for this run
            let text: String = row[run_start..run_end]
                .iter()
                .map(|c| c.c)
                .collect();

            content.add_text(&text, style);
            run_start = run_end;
        }

        // Render cursor on this row
        if row_idx == grid.cursor_row && grid.cursor_col < grid.cols {
            // Cursor is rendered as part of the content — the cursor block
            // is already included in the text above via the cell character
        }

        content.new_line();
    }

    content.build();
}
