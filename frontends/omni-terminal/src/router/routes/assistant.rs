use crate::context::grid::ContextDimension;
use terminal_backend::error::{TerminalError, TerminalErrorLevel};
use terminal_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

// Omni brand palette
const TEAL: [f32; 4] = [0.302, 0.788, 0.690, 1.0];
const TEAL_MUTED: [f32; 4] = [0.196, 0.549, 0.471, 1.0];
const TEAL_DARK: [f32; 4] = [0.118, 0.314, 0.275, 1.0];
const BG: [f32; 4] = [0.051, 0.059, 0.071, 1.0];
const AMBER: [f32; 4] = [0.706, 0.627, 0.392, 1.0];

pub struct Assistant {
    pub inner: Option<TerminalError>,
}

impl Assistant {
    pub fn new() -> Assistant {
        Assistant { inner: None }
    }

    #[inline]
    pub fn set(&mut self, report: TerminalError) {
        self.inner = Some(report);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner = None;
    }

    #[inline]
    pub fn is_warning(&self) -> bool {
        if let Some(report) = &self.inner {
            if report.level == TerminalErrorLevel::Error {
                return false;
            }
        }

        true
    }
}

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    assistant: &Assistant,
) {
    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(7);

    // Background
    objects.push(Object::Quad(Quad {
        position: [0., 0.0],
        color: BG,
        size: [
            layout.width / context_dimension.dimension.scale,
            layout.height,
        ],
        ..Quad::default()
    }));

    // Cascading teal accent bars
    objects.push(Object::Quad(Quad {
        position: [0., 30.0],
        color: TEAL,
        size: [15., layout.height],
        ..Quad::default()
    }));
    objects.push(Object::Quad(Quad {
        position: [15., context_dimension.margin.top_y + 60.],
        color: TEAL_MUTED,
        size: [15., layout.height],
        ..Quad::default()
    }));
    objects.push(Object::Quad(Quad {
        position: [30., context_dimension.margin.top_y + 120.],
        color: TEAL_DARK,
        size: [15., layout.height],
        ..Quad::default()
    }));

    let heading = sugarloaf.create_temp_rich_text();
    let action = sugarloaf.create_temp_rich_text();
    let details = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&action, 18.0);
    sugarloaf.set_rich_text_font_size(&details, 14.0);

    let content = sugarloaf.content();
    content
        .sel(heading)
        .clear()
        .add_text(
            "Omni Terminal encountered an error",
            FragmentStyle::default(),
        )
        .build();

    // Amber prompt to signal caution
    content
        .sel(action)
        .clear()
        .add_text(
            "> press enter to continue",
            FragmentStyle {
                color: AMBER,
                ..FragmentStyle::default()
            },
        )
        .build();

    if let Some(report) = &assistant.inner {
        let details_line = content.sel(details).clear();

        for line in report.report.to_string().lines() {
            details_line.add_text(line, FragmentStyle::default());
        }

        details_line.build();

        objects.push(Object::RichText(RichText {
            id: details,
            position: [70., context_dimension.margin.top_y + 140.],
            lines: None,
        }));
    }

    objects.push(Object::RichText(RichText {
        id: heading,
        position: [70., context_dimension.margin.top_y + 30.],
        lines: None,
    }));

    objects.push(Object::RichText(RichText {
        id: action,
        position: [70., context_dimension.margin.top_y + 70.],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}
