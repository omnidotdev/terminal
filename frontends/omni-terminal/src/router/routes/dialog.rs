use crate::context::grid::ContextDimension;
use terminal_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

// Omni brand palette
const TEAL: [f32; 4] = [0.302, 0.788, 0.690, 1.0];
const TEAL_MUTED: [f32; 4] = [0.196, 0.549, 0.471, 1.0];
const TEAL_DARK: [f32; 4] = [0.118, 0.314, 0.275, 1.0];
const BG: [f32; 4] = [0.051, 0.059, 0.071, 1.0];
const RED_MUTED: [f32; 4] = [0.706, 0.314, 0.314, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    heading_content: &str,
    confirm_content: &str,
    quit_content: &str,
) {
    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(7);

    // Background
    objects.push(Object::Quad(Quad {
        position: [0., 0.0],
        color: BG,
        size: [layout.width, layout.height],
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
    let confirm = sugarloaf.create_temp_rich_text();
    let quit = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&confirm, 18.0);
    sugarloaf.set_rich_text_font_size(&quit, 18.0);

    let content = sugarloaf.content();

    let heading_line = content.sel(heading).clear();
    for line in heading_content.to_string().lines() {
        heading_line.add_text(line, FragmentStyle::default());
    }
    heading_line.build();

    objects.push(Object::RichText(RichText {
        id: heading,
        position: [70., context_dimension.margin.top_y + 30.],
        lines: None,
    }));

    // Continue action (teal)
    let confirm_line = content.sel(confirm);
    confirm_line
        .clear()
        .add_text(
            &format!(" {confirm_content} "),
            FragmentStyle {
                color: BLACK,
                background_color: Some(TEAL),
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: confirm,
        position: [70., context_dimension.margin.top_y + 100.],
        lines: None,
    }));

    // Quit action (muted red)
    let quit_line = content.sel(quit);
    quit_line
        .clear()
        .add_text(
            &format!(" {quit_content} "),
            FragmentStyle {
                color: BLACK,
                background_color: Some(RED_MUTED),
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: quit,
        position: [70., context_dimension.margin.top_y + 140.],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}
