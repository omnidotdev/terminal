use crate::context::grid::ContextDimension;
use terminal_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

// Omni brand palette
const TEAL: [f32; 4] = [0.302, 0.788, 0.690, 1.0];
const TEAL_MUTED: [f32; 4] = [0.196, 0.549, 0.471, 1.0];
const TEAL_DARK: [f32; 4] = [0.118, 0.314, 0.275, 1.0];
const BG: [f32; 4] = [0.051, 0.059, 0.071, 1.0];
const DIMMED: [f32; 4] = [0.392, 0.392, 0.431, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(10);

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

    let logo_shadow = sugarloaf.create_temp_rich_text();
    let logo = sugarloaf.create_temp_rich_text();
    let subtitle = sugarloaf.create_temp_rich_text();
    let action = sugarloaf.create_temp_rich_text();
    let info = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&logo_shadow, 24.0);
    sugarloaf.set_rich_text_font_size(&logo, 24.0);
    sugarloaf.set_rich_text_font_size(&subtitle, 18.0);
    sugarloaf.set_rich_text_font_size(&action, 18.0);
    sugarloaf.set_rich_text_font_size(&info, 16.0);

    let content = sugarloaf.content();

    // Logo shadow (rendered behind, offset slightly)
    let shadow_style = FragmentStyle {
        color: TEAL_DARK,
        ..FragmentStyle::default()
    };
    content
        .sel(logo_shadow)
        .clear()
        .add_text("█▀▀█ █▀▄▀█ █▀▀▄ ▀█▀", shadow_style)
        .new_line()
        .add_text("█  █ █ █ █ █  █  █", shadow_style)
        .new_line()
        .add_text("▀▀▀▀ ▀   ▀ ▀  ▀ ▀▀▀", shadow_style)
        .build();

    // Logo (main, teal)
    let logo_style = FragmentStyle {
        color: TEAL,
        ..FragmentStyle::default()
    };
    content
        .sel(logo)
        .clear()
        .add_text("█▀▀█ █▀▄▀█ █▀▀▄ ▀█▀", logo_style)
        .new_line()
        .add_text("█  █ █ █ █ █  █  █", logo_style)
        .new_line()
        .add_text("▀▀▀▀ ▀   ▀ ▀  ▀ ▀▀▀", logo_style)
        .build();

    // Subtitle
    content
        .sel(subtitle)
        .clear()
        .add_text(
            "Terminal",
            FragmentStyle {
                color: DIMMED,
                ..FragmentStyle::default()
            },
        )
        .build();

    // Action prompt
    content
        .sel(action)
        .clear()
        .add_text(
            "> press enter to continue",
            FragmentStyle {
                color: TEAL,
                ..FragmentStyle::default()
            },
        )
        .build();

    // Config + shortcut info
    #[cfg(target_os = "macos")]
    let shortcut = "\"Command\" + \",\" (comma)";

    #[cfg(not(target_os = "macos"))]
    let shortcut = "\"Control\" + \"Shift\" + \",\" (comma)";

    content
        .sel(info)
        .clear()
        .add_text(
            "Your configuration file will be created in",
            FragmentStyle {
                color: DIMMED,
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text(
            &format!(" {} ", terminal_backend::config::config_file_path().display()),
            FragmentStyle {
                background_color: Some(TEAL),
                color: BLACK,
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text(
            "To open settings menu use",
            FragmentStyle {
                color: DIMMED,
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text(
            &format!(" {shortcut} "),
            FragmentStyle {
                background_color: Some(TEAL),
                color: BLACK,
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text(
            "terminal.omni.dev",
            FragmentStyle {
                color: DIMMED,
                ..FragmentStyle::default()
            },
        )
        .build();

    // Position objects: shadow slightly offset behind logo
    objects.push(Object::RichText(RichText {
        id: logo_shadow,
        position: [72., context_dimension.margin.top_y + 32.],
        lines: None,
    }));
    objects.push(Object::RichText(RichText {
        id: logo,
        position: [70., context_dimension.margin.top_y + 30.],
        lines: None,
    }));
    objects.push(Object::RichText(RichText {
        id: subtitle,
        position: [70., context_dimension.margin.top_y + 110.],
        lines: None,
    }));
    objects.push(Object::RichText(RichText {
        id: action,
        position: [70., context_dimension.margin.top_y + 150.],
        lines: None,
    }));
    objects.push(Object::RichText(RichText {
        id: info,
        position: [70., context_dimension.margin.top_y + 200.],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}
