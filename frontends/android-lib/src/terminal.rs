use copa::{Params, Perform};

/// Terminal cell with character and style attributes
#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: [f32; 4],
    pub bg: Option<[f32; 4]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: [1.0, 1.0, 1.0, 1.0],
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum MouseMode {
    None,
    Click,
    DragMotion,
    AllMotion,
}

/// Maximum number of lines kept in scrollback history.
const MAX_SCROLLBACK: usize = 1000;

/// Simple terminal grid state driven by ANSI escape sequences
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<Cell>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub dirty: bool,

    // Scrollback history (oldest first)
    scrollback: Vec<Vec<Cell>>,
    /// Viewport offset from the bottom. 0 = viewing live output.
    pub display_offset: usize,

    // Current text attributes
    cur_fg: [f32; 4],
    cur_bg: Option<[f32; 4]>,
    cur_bold: bool,
    cur_italic: bool,
    cur_underline: bool,
    cur_inverse: bool,

    // Scroll region
    scroll_top: usize,
    scroll_bottom: usize,

    // Saved cursor position
    saved_cursor_row: usize,
    saved_cursor_col: usize,

    // Mouse reporting modes (DECSET)
    mouse_click: bool,  // Mode 1000: report clicks
    mouse_drag: bool,   // Mode 1002: report drag motion
    mouse_motion: bool, // Mode 1003: report all motion
    mouse_sgr: bool,    // Mode 1006: SGR extended encoding

    // Bytes to send back to the PTY (mouse reports, etc.). Drained by lib.rs each frame.
    #[allow(dead_code)]
    pub pending_writes: Vec<u8>,
}

impl TerminalGrid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            cols,
            rows,
            cells,
            cursor_row: 0,
            cursor_col: 0,
            dirty: true,
            scrollback: Vec::new(),
            display_offset: 0,
            cur_fg: [1.0, 1.0, 1.0, 1.0],
            cur_bg: None,
            cur_bold: false,
            cur_italic: false,
            cur_underline: false,
            cur_inverse: false,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            mouse_click: false,
            mouse_drag: false,
            mouse_motion: false,
            mouse_sgr: false,
            pending_writes: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn mouse_mode(&self) -> MouseMode {
        if self.mouse_motion {
            MouseMode::AllMotion
        } else if self.mouse_drag {
            MouseMode::DragMotion
        } else if self.mouse_click {
            MouseMode::Click
        } else {
            MouseMode::None
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.cells.resize(rows, vec![Cell::default(); cols]);
        for row in &mut self.cells {
            row.resize(cols, Cell::default());
        }
        self.scroll_bottom = rows - 1;
        if self.cursor_row >= rows {
            self.cursor_row = rows - 1;
        }
        if self.cursor_col >= cols {
            self.cursor_col = cols - 1;
        }
        self.dirty = true;
    }

    /// Adjust the viewport by `delta` lines. Positive = scroll up (into history).
    pub fn scroll_display(&mut self, delta: i32) {
        let max = self.scrollback.len();
        let new_offset = (self.display_offset as i32 + delta).clamp(0, max as i32);
        self.display_offset = new_offset as usize;
        self.dirty = true;
    }

    /// Return the row to display at screen position `row_idx`, accounting for
    /// `display_offset`. When scrolled back, rows come from scrollback history.
    pub fn visible_row(&self, row_idx: usize) -> &Vec<Cell> {
        if self.display_offset == 0 {
            return &self.cells[row_idx];
        }

        // Total virtual lines = scrollback + live cells
        // We want to show `rows` lines ending at (total - display_offset)
        let total = self.scrollback.len() + self.rows;
        let end = total - self.display_offset;
        let start = end.saturating_sub(self.rows);
        let abs_idx = start + row_idx;

        if abs_idx < self.scrollback.len() {
            &self.scrollback[abs_idx]
        } else {
            &self.cells[abs_idx - self.scrollback.len()]
        }
    }

    fn scroll_up(&mut self) {
        let removed = self.cells.remove(self.scroll_top);
        // Only save to scrollback when the whole screen scrolls (region == full screen)
        if self.scroll_top == 0 {
            self.scrollback.push(removed);
            if self.scrollback.len() > MAX_SCROLLBACK {
                self.scrollback.remove(0);
            }
        }
        self.cells
            .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
        self.dirty = true;
    }

    fn scroll_down(&mut self) {
        self.cells.remove(self.scroll_bottom);
        self.cells
            .insert(self.scroll_top, vec![Cell::default(); self.cols]);
        self.dirty = true;
    }

    fn new_cell(&self, c: char) -> Cell {
        Cell {
            c,
            fg: self.cur_fg,
            bg: self.cur_bg,
            bold: self.cur_bold,
            italic: self.cur_italic,
            underline: self.cur_underline,
            inverse: self.cur_inverse,
        }
    }

    fn clear_row(&mut self, row: usize) {
        if row < self.rows {
            self.cells[row] = vec![Cell::default(); self.cols];
        }
    }

    fn erase_in_display(&mut self, mode: u16) {
        match mode {
            // Clear from cursor to end of screen
            0 => {
                // Clear rest of current row
                for col in self.cursor_col..self.cols {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
                // Clear rows below
                for row in (self.cursor_row + 1)..self.rows {
                    self.clear_row(row);
                }
            }
            // Clear from beginning to cursor
            1 => {
                for row in 0..self.cursor_row {
                    self.clear_row(row);
                }
                for col in 0..=self.cursor_col.min(self.cols - 1) {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            // Clear entire screen
            2 | 3 => {
                for row in 0..self.rows {
                    self.clear_row(row);
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn erase_in_line(&mut self, mode: u16) {
        match mode {
            // Clear from cursor to end of line
            0 => {
                for col in self.cursor_col..self.cols {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            // Clear from beginning to cursor
            1 => {
                for col in 0..=self.cursor_col.min(self.cols - 1) {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            // Clear entire line
            2 => {
                self.clear_row(self.cursor_row);
            }
            _ => {}
        }
        self.dirty = true;
    }
}

// Standard 256-color palette (first 16 colors)
fn ansi_color(idx: u16) -> [f32; 4] {
    match idx {
        0 => [0.0, 0.0, 0.0, 1.0],       // Black
        1 => [0.8, 0.0, 0.0, 1.0],        // Red
        2 => [0.0, 0.8, 0.0, 1.0],        // Green
        3 => [0.8, 0.8, 0.0, 1.0],        // Yellow
        4 => [0.0, 0.0, 0.8, 1.0],        // Blue
        5 => [0.8, 0.0, 0.8, 1.0],        // Magenta
        6 => [0.0, 0.8, 0.8, 1.0],        // Cyan
        7 => [0.75, 0.75, 0.75, 1.0],     // White
        8 => [0.5, 0.5, 0.5, 1.0],        // Bright black
        9 => [1.0, 0.0, 0.0, 1.0],        // Bright red
        10 => [0.0, 1.0, 0.0, 1.0],       // Bright green
        11 => [1.0, 1.0, 0.0, 1.0],       // Bright yellow
        12 => [0.0, 0.0, 1.0, 1.0],       // Bright blue
        13 => [1.0, 0.0, 1.0, 1.0],       // Bright magenta
        14 => [0.0, 1.0, 1.0, 1.0],       // Bright cyan
        15 => [1.0, 1.0, 1.0, 1.0],       // Bright white
        16..=231 => {
            // 6x6x6 color cube
            let idx = idx - 16;
            let r = (idx / 36) as f32 / 5.0;
            let g = ((idx % 36) / 6) as f32 / 5.0;
            let b = (idx % 6) as f32 / 5.0;
            [r, g, b, 1.0]
        }
        232..=255 => {
            // Grayscale ramp
            let level = (idx - 232) as f32 / 23.0;
            [level, level, level, 1.0]
        }
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

impl Perform for TerminalGrid {
    fn print(&mut self, c: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.cursor_row += 1;
            if self.cursor_row > self.scroll_bottom {
                self.cursor_row = self.scroll_bottom;
                self.scroll_up();
            }
        }

        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.cells[self.cursor_row][self.cursor_col] = self.new_cell(c);
            self.cursor_col += 1;
        }
        self.dirty = true;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // Bell
            0x07 => {}
            // Backspace
            0x08 => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            // Tab
            0x09 => {
                let next_tab = (self.cursor_col / 8 + 1) * 8;
                self.cursor_col = next_tab.min(self.cols - 1);
            }
            // Line feed / Vertical tab / Form feed
            0x0A | 0x0B | 0x0C => {
                self.cursor_row += 1;
                if self.cursor_row > self.scroll_bottom {
                    self.cursor_row = self.scroll_bottom;
                    self.scroll_up();
                }
            }
            // Carriage return
            0x0D => {
                self.cursor_col = 0;
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let mut param_iter = params.iter();
        let first = param_iter
            .next()
            .and_then(|p| p.first().copied())
            .unwrap_or(0);

        match action {
            // Cursor Up
            'A' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            // Cursor Down
            'B' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
            }
            // Cursor Forward
            'C' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
            }
            // Cursor Back
            'D' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            // Cursor Next Line
            'E' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                self.cursor_col = 0;
            }
            // Cursor Previous Line
            'F' => {
                let n = if first == 0 { 1 } else { first as usize };
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            // Cursor Horizontal Absolute
            'G' => {
                let col = if first == 0 { 1 } else { first as usize };
                self.cursor_col = (col - 1).min(self.cols - 1);
            }
            // Cursor Position
            'H' | 'f' => {
                let row = if first == 0 { 1 } else { first as usize };
                let col = param_iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1) as usize;
                let col = if col == 0 { 1 } else { col };
                self.cursor_row = (row - 1).min(self.rows - 1);
                self.cursor_col = (col - 1).min(self.cols - 1);
            }
            // Erase in Display
            'J' => {
                self.erase_in_display(first);
            }
            // Erase in Line
            'K' => {
                self.erase_in_line(first);
            }
            // Insert Lines
            'L' => {
                let n = if first == 0 { 1 } else { first as usize };
                for _ in 0..n {
                    if self.cursor_row <= self.scroll_bottom {
                        self.cells.remove(self.scroll_bottom);
                        self.cells
                            .insert(self.cursor_row, vec![Cell::default(); self.cols]);
                    }
                }
                self.dirty = true;
            }
            // Delete Lines
            'M' => {
                let n = if first == 0 { 1 } else { first as usize };
                for _ in 0..n {
                    if self.cursor_row <= self.scroll_bottom {
                        self.cells.remove(self.cursor_row);
                        self.cells
                            .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
                    }
                }
                self.dirty = true;
            }
            // Delete Characters
            'P' => {
                let n = if first == 0 { 1 } else { first as usize };
                let row = &mut self.cells[self.cursor_row];
                for _ in 0..n.min(self.cols - self.cursor_col) {
                    if self.cursor_col < row.len() {
                        row.remove(self.cursor_col);
                        row.push(Cell::default());
                    }
                }
                self.dirty = true;
            }
            // Scroll Up
            'S' => {
                let n = if first == 0 { 1 } else { first as usize };
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            // Scroll Down
            'T' => {
                let n = if first == 0 { 1 } else { first as usize };
                for _ in 0..n {
                    self.scroll_down();
                }
            }
            // Insert Characters
            '@' => {
                let n = if first == 0 { 1 } else { first as usize };
                let row = &mut self.cells[self.cursor_row];
                for _ in 0..n.min(self.cols - self.cursor_col) {
                    row.insert(self.cursor_col, Cell::default());
                    row.truncate(self.cols);
                }
                self.dirty = true;
            }
            // SGR - Select Graphic Rendition
            'm' => {
                self.handle_sgr(params);
            }
            // Set Scrolling Region
            'r' => {
                let top = if first == 0 { 1 } else { first as usize };
                let bottom = param_iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .map(|b| if b == 0 { self.rows } else { b as usize })
                    .unwrap_or(self.rows);
                self.scroll_top = (top - 1).min(self.rows - 1);
                self.scroll_bottom = (bottom - 1).min(self.rows - 1);
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            // DECSET (private mode set)
            'h' if intermediates == [b'?'] => {
                for sub in params.iter() {
                    match sub.first().copied().unwrap_or(0) {
                        1000 => {
                            self.mouse_click = true;
                            self.mouse_drag = false;
                            self.mouse_motion = false;
                        }
                        1002 => {
                            self.mouse_click = false;
                            self.mouse_drag = true;
                            self.mouse_motion = false;
                        }
                        1003 => {
                            self.mouse_click = false;
                            self.mouse_drag = false;
                            self.mouse_motion = true;
                        }
                        1006 => {
                            self.mouse_sgr = true;
                        }
                        _ => {}
                    }
                }
            }
            // DECRST (private mode reset)
            'l' if intermediates == [b'?'] => {
                for sub in params.iter() {
                    match sub.first().copied().unwrap_or(0) {
                        1000 => self.mouse_click = false,
                        1002 => self.mouse_drag = false,
                        1003 => self.mouse_motion = false,
                        1006 => self.mouse_sgr = false,
                        _ => {}
                    }
                }
            }
            // Non-private set/reset (ignore)
            'h' | 'l' => {}
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (byte, intermediates) {
            // Save cursor
            (b'7', _) | (b's', _) => {
                self.saved_cursor_row = self.cursor_row;
                self.saved_cursor_col = self.cursor_col;
            }
            // Restore cursor
            (b'8', _) | (b'u', _) => {
                self.cursor_row = self.saved_cursor_row;
                self.cursor_col = self.saved_cursor_col;
            }
            // Reverse Index (scroll down if at top)
            (b'M', _) => {
                if self.cursor_row == self.scroll_top {
                    self.scroll_down();
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences (title, colors, etc.) — not needed for basic terminal
    }
}

impl TerminalGrid {
    fn handle_sgr(&mut self, params: &Params) {
        let params_vec: Vec<u16> = params
            .iter()
            .flat_map(|subparams| subparams.iter().copied())
            .collect();

        if params_vec.is_empty() {
            self.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < params_vec.len() {
            match params_vec[i] {
                0 => self.reset_attributes(),
                1 => self.cur_bold = true,
                3 => self.cur_italic = true,
                4 => self.cur_underline = true,
                7 => self.cur_inverse = true,
                22 => self.cur_bold = false,
                23 => self.cur_italic = false,
                24 => self.cur_underline = false,
                27 => self.cur_inverse = false,
                // Foreground colors
                30..=37 => self.cur_fg = ansi_color(params_vec[i] - 30),
                38 => {
                    if i + 1 < params_vec.len() {
                        match params_vec[i + 1] {
                            5 if i + 2 < params_vec.len() => {
                                self.cur_fg = ansi_color(params_vec[i + 2]);
                                i += 2;
                            }
                            2 if i + 4 < params_vec.len() => {
                                let r = params_vec[i + 2] as f32 / 255.0;
                                let g = params_vec[i + 3] as f32 / 255.0;
                                let b = params_vec[i + 4] as f32 / 255.0;
                                self.cur_fg = [r, g, b, 1.0];
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.cur_fg = [1.0, 1.0, 1.0, 1.0], // Default fg
                // Background colors
                40..=47 => self.cur_bg = Some(ansi_color(params_vec[i] - 40)),
                48 => {
                    if i + 1 < params_vec.len() {
                        match params_vec[i + 1] {
                            5 if i + 2 < params_vec.len() => {
                                self.cur_bg = Some(ansi_color(params_vec[i + 2]));
                                i += 2;
                            }
                            2 if i + 4 < params_vec.len() => {
                                let r = params_vec[i + 2] as f32 / 255.0;
                                let g = params_vec[i + 3] as f32 / 255.0;
                                let b = params_vec[i + 4] as f32 / 255.0;
                                self.cur_bg = Some([r, g, b, 1.0]);
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.cur_bg = None, // Default bg
                // Bright foreground
                90..=97 => self.cur_fg = ansi_color(params_vec[i] - 90 + 8),
                // Bright background
                100..=107 => {
                    self.cur_bg = Some(ansi_color(params_vec[i] - 100 + 8))
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn reset_attributes(&mut self) {
        self.cur_fg = [1.0, 1.0, 1.0, 1.0];
        self.cur_bg = None;
        self.cur_bold = false;
        self.cur_italic = false;
        self.cur_underline = false;
        self.cur_inverse = false;
    }

    /// Generate an SGR mouse report and push it to pending_writes.
    ///
    /// Only SGR encoding (mode 1006) is supported; legacy X10 encoding is not implemented.
    /// `button` uses X11 convention: 0=left, 1=middle, 2=right, 64=wheel_up, 65=wheel_down.
    /// `modifiers` is a bitmask: 4=shift, 8=alt, 16=ctrl.
    /// `col` and `row` are 0-indexed grid coordinates.
    /// `pressed` is true for press/motion, false for release.
    #[allow(dead_code)]
    pub fn mouse_report(
        &mut self,
        button: u8,
        modifiers: u8,
        col: usize,
        row: usize,
        pressed: bool,
    ) {
        if self.mouse_mode() == MouseMode::None || !self.mouse_sgr {
            return;
        }

        let col = col.min(self.cols.saturating_sub(1));
        let row = row.min(self.rows.saturating_sub(1));
        let cb = button | modifiers;
        let suffix = if pressed { 'M' } else { 'm' };
        let seq = format!("\x1b[<{};{};{}{}", cb, col + 1, row + 1, suffix);
        self.pending_writes.extend_from_slice(seq.as_bytes());
    }
}
