pub const MAX_SCROLLBACK: usize = 1000;

#[derive(Clone)]
pub struct Cell {
    pub character: char,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseMode {
    Off,
}

pub struct TerminalGrid {
    _placeholder: (),
}
