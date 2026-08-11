// pattern: Functional Core

use std::fmt::Write;

const FULL_WIDTH: u16 = 100;
const COMPACT_WIDTH: u16 = 72;
const TAGLINE: &str = "Inspect metadata. Decode the reconstruction basis.";
const ANSI_RESET: &str = "\x1b[0m";
const SUPPORTING_COPY_COLOR: Rgb = Rgb(0x77, 0xd6, 0x5a);
const GRADIENT_STOPS: [Rgb; 4] = [
    Rgb(0x77, 0xd6, 0x5a),
    Rgb(0x20, 0xd6, 0xb5),
    Rgb(0x32, 0x95, 0xe8),
    Rgb(0x8a, 0x55, 0xe8),
];

const FULL_ART: [&str; 11] = [
    "      ___           ___         ___           ___                      ___           ___",
    "     /\\  \\         /\\  \\       /\\__\\         /\\  \\        ___         /\\  \\         /\\__\\",
    "    /::\\  \\       /::\\  \\     /:/ _/_        \\:\\  \\      /\\__\\       /::\\  \\       /:/  /",
    "   /:/\\:\\  \\     /:/\\:\\__\\   /:/ /\\__\\        \\:\\  \\    /:/__/      /:/\\:\\  \\     /:/  /",
    "  /:/  \\:\\  \\   /:/ /:/  /  /:/ /:/ _/_   _____\\:\\  \\  /::\\  \\     /:/  \\:\\  \\   /:/  /  ___",
    " /:/__/ \\:\\__\\ /:/_/:/  /  /:/_/:/ /\\__\\ /::::::::\\__\\ \\/\\:\\  \\   /:/__/ \\:\\__\\ /:/__/  /\\__\\",
    " \\:\\  \\ /:/  / \\:\\/:/  /   \\:\\/:/ /:/  / \\:\\__\\__\\/__/    \\:\\  \\  \\:\\  \\ /:/  / \\:\\  \\ /:/  /",
    "  \\:\\  /:/  /   \\::/__/     \\::/_/:/  /   \\:\\  \\           \\:\\__\\  \\:\\  /:/  /   \\:\\  /:/  /",
    "   \\:\\/:/  /     \\:\\  \\      \\:\\/:/  /     \\:\\  \\          /:/  /   \\:\\/:/  /     \\:\\/:/  /",
    "    \\::/  /       \\:\\__\\      \\::/  /       \\:\\__\\        /:/  /     \\::/  /       \\::/  /",
    "     \\/__/         \\/__/       \\/__/         \\/__/        \\/__/       \\/__/         \\/__/",
];

const COMPACT_ART: [&str; 6] = [
    "   ___                       _  ___   ____",
    "  / _ \\ _ __   ___ _ __     | |/ _ \\ / ___|",
    " | | | | '_ \\ / _ \\ '_ \\ _  | | | | | |",
    " | |_| | |_) |  __/ | | | | |_| | |_| | |___",
    "  \\___/| .__/ \\___|_| |_|\\___/ \\___/ \\____|",
    "       |_|",
];

#[derive(Clone, Copy)]
struct Rgb(u8, u8, u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BannerMode {
    Hidden,
    Minimal,
    Compact,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// These independent facts are deliberately explicit so the visibility policy
// can be tested without reading process or terminal state.
#[allow(clippy::struct_excessive_bools)]
pub struct BannerContext {
    pub is_tty: bool,
    pub terminal_width: Option<u16>,
    pub no_color: bool,
    pub no_banner: bool,
    pub term_is_dumb: bool,
    pub is_root_help: bool,
    pub is_root_without_task: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectMetadata<'a> {
    pub version: &'a str,
    pub license: &'a str,
    pub description: &'a str,
}

pub const fn package_metadata() -> ProjectMetadata<'static> {
    ProjectMetadata {
        version: env!("CARGO_PKG_VERSION"),
        license: env!("CARGO_PKG_LICENSE"),
        description: env!("CARGO_PKG_DESCRIPTION"),
    }
}

pub fn select_mode(context: BannerContext) -> BannerMode {
    if context.no_banner
        || !context.is_tty
        || (!context.is_root_help && !context.is_root_without_task)
    {
        return BannerMode::Hidden;
    }
    if context.term_is_dumb {
        return BannerMode::Minimal;
    }
    match context.terminal_width {
        Some(width) if width >= FULL_WIDTH => BannerMode::Full,
        Some(width) if width >= COMPACT_WIDTH => BannerMode::Compact,
        _ => BannerMode::Minimal,
    }
}

pub fn render_banner(context: BannerContext, metadata: ProjectMetadata<'_>) -> String {
    match select_mode(context) {
        BannerMode::Hidden => String::new(),
        BannerMode::Minimal => render_minimal(context.terminal_width, metadata),
        BannerMode::Compact => render_compact(context.no_color, metadata),
        BannerMode::Full => render_full(context.no_color, metadata),
    }
}

fn render_full(no_color: bool, metadata: ProjectMetadata<'_>) -> String {
    let mut output = render_ascii(&FULL_ART, !no_color);
    output.push('\n');
    push_supporting_line(&mut output, metadata.description, !no_color);
    push_supporting_line(&mut output, TAGLINE, !no_color);
    output.push('\n');
    writeln!(
        output,
        "Version: {}  |  License: {}",
        metadata.version, metadata.license
    )
    .expect("writing to a String cannot fail");
    output
}

fn render_compact(no_color: bool, metadata: ProjectMetadata<'_>) -> String {
    let mut output = render_ascii(&COMPACT_ART, !no_color);
    output.push('\n');
    push_supporting_line(&mut output, TAGLINE, !no_color);
    writeln!(output, "{}  |  {}", metadata.version, metadata.license)
        .expect("writing to a String cannot fail");
    output
}

fn render_minimal(width: Option<u16>, metadata: ProjectMetadata<'_>) -> String {
    let available = width.map_or(0, usize::from);
    let title = format!("OpenJOC {}", metadata.version);
    let mut output = String::new();
    if width.is_none() || available >= title.len() {
        push_line(&mut output, &title);
    } else if available >= "OpenJOC".len() {
        push_line(&mut output, "OpenJOC");
    } else {
        return output;
    }
    if available >= TAGLINE.len() {
        push_line(&mut output, TAGLINE);
    }
    output
}

fn render_ascii<const N: usize>(lines: &[&str; N], color: bool) -> String {
    let span = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1);
    let mut output = String::new();
    for line in lines {
        if color {
            render_gradient_line(&mut output, line, span);
        } else {
            push_line(&mut output, line);
        }
    }
    output
}

fn render_gradient_line(output: &mut String, line: &str, span: usize) {
    for (column, character) in line.chars().enumerate() {
        if character == ' ' {
            output.push(character);
            continue;
        }
        let Rgb(red, green, blue) = gradient_color(column, span);
        write!(output, "\x1b[38;2;{red};{green};{blue}m{character}")
            .expect("writing to a String cannot fail");
    }
    output.push_str(ANSI_RESET);
    output.push('\n');
}

fn gradient_color(column: usize, span: usize) -> Rgb {
    let denominator = span.saturating_sub(1).max(1);
    let scaled = column.saturating_mul(GRADIENT_STOPS.len() - 1);
    let segment = (scaled / denominator).min(GRADIENT_STOPS.len() - 2);
    let remainder = scaled.saturating_sub(segment * denominator);
    interpolate(
        GRADIENT_STOPS[segment],
        GRADIENT_STOPS[segment + 1],
        remainder,
        denominator,
    )
}

fn interpolate(start: Rgb, end: Rgb, position: usize, length: usize) -> Rgb {
    Rgb(
        interpolate_channel(start.0, end.0, position, length),
        interpolate_channel(start.1, end.1, position, length),
        interpolate_channel(start.2, end.2, position, length),
    )
}

fn interpolate_channel(start: u8, end: u8, position: usize, length: usize) -> u8 {
    let position = u32::try_from(position).expect("banner width fits in u32");
    let length = u32::try_from(length).expect("banner width fits in u32");
    let remaining = length - position;
    let value = u32::from(start) * remaining + u32::from(end) * position;
    u8::try_from((value + length / 2) / length).expect("interpolated RGB channel fits in u8")
}

fn push_line(output: &mut String, line: &str) {
    output.push_str(line);
    output.push('\n');
}

fn push_supporting_line(output: &mut String, line: &str, color: bool) {
    if color {
        let Rgb(red, green, blue) = SUPPORTING_COPY_COLOR;
        writeln!(output, "\x1b[38;2;{red};{green};{blue}m{line}{ANSI_RESET}")
            .expect("writing to a String cannot fail");
    } else {
        push_line(output, line);
    }
}

#[cfg(test)]
mod tests {
    use super::{BannerContext, BannerMode, ProjectMetadata, render_banner, select_mode};

    const METADATA: ProjectMetadata<'_> = ProjectMetadata {
        version: "1.2.3-dev",
        license: "Apache-2.0",
        description: "Research-grade E-AC-3 JOC metadata and reconstruction-basis decoder",
    };

    fn root_context(width: u16) -> BannerContext {
        BannerContext {
            is_tty: true,
            terminal_width: Some(width),
            no_color: false,
            no_banner: false,
            term_is_dumb: false,
            is_root_help: false,
            is_root_without_task: true,
        }
    }

    #[test]
    fn banner_is_hidden_outside_safe_interactive_root_scenarios() {
        let mut context = root_context(120);
        context.is_tty = false;
        assert_eq!(select_mode(context), BannerMode::Hidden);

        context = root_context(120);
        context.no_banner = true;
        assert_eq!(select_mode(context), BannerMode::Hidden);

        context = root_context(120);
        context.is_root_without_task = false;
        assert_eq!(select_mode(context), BannerMode::Hidden);
    }

    #[test]
    fn root_help_is_an_allowed_banner_scenario() {
        let mut context = root_context(120);
        context.is_root_without_task = false;
        context.is_root_help = true;
        assert_eq!(select_mode(context), BannerMode::Full);
    }

    #[test]
    fn width_and_terminal_capabilities_select_safe_layouts() {
        assert_eq!(select_mode(root_context(100)), BannerMode::Full);
        assert_eq!(select_mode(root_context(99)), BannerMode::Compact);
        assert_eq!(select_mode(root_context(72)), BannerMode::Compact);
        assert_eq!(select_mode(root_context(71)), BannerMode::Minimal);

        let mut context = root_context(120);
        context.terminal_width = None;
        assert_eq!(select_mode(context), BannerMode::Minimal);

        context = root_context(120);
        context.term_is_dumb = true;
        assert_eq!(select_mode(context), BannerMode::Minimal);
    }

    #[test]
    fn full_plain_banner_matches_supplied_3d_art_golden_text() {
        let mut context = root_context(120);
        context.no_color = true;
        assert_eq!(
            render_banner(context, METADATA),
            concat!(
                "      ___           ___         ___           ___                      ___           ___\n",
                "     /\\  \\         /\\  \\       /\\__\\         /\\  \\        ___         /\\  \\         /\\__\\\n",
                "    /::\\  \\       /::\\  \\     /:/ _/_        \\:\\  \\      /\\__\\       /::\\  \\       /:/  /\n",
                "   /:/\\:\\  \\     /:/\\:\\__\\   /:/ /\\__\\        \\:\\  \\    /:/__/      /:/\\:\\  \\     /:/  /\n",
                "  /:/  \\:\\  \\   /:/ /:/  /  /:/ /:/ _/_   _____\\:\\  \\  /::\\  \\     /:/  \\:\\  \\   /:/  /  ___\n",
                " /:/__/ \\:\\__\\ /:/_/:/  /  /:/_/:/ /\\__\\ /::::::::\\__\\ \\/\\:\\  \\   /:/__/ \\:\\__\\ /:/__/  /\\__\\\n",
                " \\:\\  \\ /:/  / \\:\\/:/  /   \\:\\/:/ /:/  / \\:\\__\\__\\/__/    \\:\\  \\  \\:\\  \\ /:/  / \\:\\  \\ /:/  /\n",
                "  \\:\\  /:/  /   \\::/__/     \\::/_/:/  /   \\:\\  \\           \\:\\__\\  \\:\\  /:/  /   \\:\\  /:/  /\n",
                "   \\:\\/:/  /     \\:\\  \\      \\:\\/:/  /     \\:\\  \\          /:/  /   \\:\\/:/  /     \\:\\/:/  /\n",
                "    \\::/  /       \\:\\__\\      \\::/  /       \\:\\__\\        /:/  /     \\::/  /       \\::/  /\n",
                "     \\/__/         \\/__/       \\/__/         \\/__/        \\/__/       \\/__/         \\/__/\n",
                "\n",
                "Research-grade E-AC-3 JOC metadata and reconstruction-basis decoder\n",
                "Inspect metadata. Decode the reconstruction basis.\n",
                "\n",
                "Version: 1.2.3-dev  |  License: Apache-2.0\n",
            )
        );
    }

    #[test]
    fn compact_plain_banner_matches_reference_style_golden_text() {
        let mut context = root_context(80);
        context.no_color = true;
        assert_eq!(
            render_banner(context, METADATA),
            concat!(
                "   ___                       _  ___   ____\n",
                "  / _ \\ _ __   ___ _ __     | |/ _ \\ / ___|\n",
                " | | | | '_ \\ / _ \\ '_ \\ _  | | | | | |\n",
                " | |_| | |_) |  __/ | | | | |_| | |_| | |___\n",
                "  \\___/| .__/ \\___|_| |_|\\___/ \\___/ \\____|\n",
                "       |_|\n",
                "\n",
                "Inspect metadata. Decode the reconstruction basis.\n",
                "1.2.3-dev  |  Apache-2.0\n",
            )
        );
    }

    #[test]
    fn minimal_banner_drops_the_tagline_before_it_can_wrap() {
        let mut context = root_context(60);
        context.no_color = true;
        assert_eq!(
            render_banner(context, METADATA),
            "OpenJOC 1.2.3-dev\nInspect metadata. Decode the reconstruction basis.\n"
        );

        context.terminal_width = Some(20);
        assert_eq!(render_banner(context, METADATA), "OpenJOC 1.2.3-dev\n");

        context.terminal_width = Some(10);
        assert_eq!(render_banner(context, METADATA), "OpenJOC\n");
    }

    #[test]
    fn hidden_and_no_color_rendering_never_emit_ansi() {
        let mut context = root_context(120);
        context.no_banner = true;
        assert!(render_banner(context, METADATA).is_empty());

        context.no_banner = false;
        context.no_color = true;
        assert!(!render_banner(context, METADATA).contains("\x1b["));
    }

    #[test]
    fn every_colored_ascii_line_ends_with_a_reset() {
        for (width, expected_colored_lines) in [(120, 13), (80, 7)] {
            let output = render_banner(root_context(width), METADATA);
            let colored_lines = output
                .lines()
                .filter(|line| line.contains("\x1b[38;2;"))
                .collect::<Vec<_>>();
            assert_eq!(colored_lines.len(), expected_colored_lines);
            assert!(colored_lines.iter().all(|line| line.ends_with("\x1b[0m")));
        }
    }

    #[test]
    fn colored_full_banner_uses_a_single_green_for_supporting_copy() {
        let output = render_banner(root_context(120), METADATA);
        assert!(output.contains(concat!(
            "\x1b[38;2;119;214;90m",
            "Research-grade E-AC-3 JOC metadata and reconstruction-basis decoder",
            "\x1b[0m"
        )));
        assert!(output.contains(concat!(
            "\x1b[38;2;119;214;90m",
            "Inspect metadata. Decode the reconstruction basis.",
            "\x1b[0m"
        )));
    }

    #[test]
    fn plain_banner_lines_fit_the_selected_terminal_width() {
        for width in [100, 120, 72, 80, 36, 71] {
            let mut context = root_context(width);
            context.no_color = true;
            let output = render_banner(context, METADATA);
            assert!(
                output
                    .lines()
                    .all(|line| line.chars().count() <= usize::from(width)),
                "banner for width {width} exceeded its terminal width:\n{output}"
            );
        }
    }
}
