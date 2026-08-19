// pattern: Imperative Shell

use crate::banner::BannerContext;
use std::{env, io::IsTerminal};
use terminal_size::Width;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// TTY, color, banner, and TERM controls are independent environment facts.
#[allow(clippy::struct_excessive_bools)]
pub struct TerminalCapabilities {
    pub is_tty: bool,
    pub stderr_is_tty: bool,
    pub width: Option<u16>,
    pub no_color: bool,
    pub no_banner: bool,
    pub term_is_dumb: bool,
}

impl TerminalCapabilities {
    pub fn detect() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let stderr_is_tty = std::io::stderr().is_terminal();
        let width = if is_tty || stderr_is_tty {
            terminal_size::terminal_size()
                .map(|(Width(width), _)| width)
                .or_else(|| {
                    env::var("COLUMNS")
                        .ok()
                        .and_then(|value| value.parse::<u16>().ok())
                        .filter(|width| *width > 0)
                })
        } else {
            None
        };
        let no_banner = env::var("OPENJOC_NO_BANNER").ok();
        let term = env::var("TERM").ok();
        Self::from_inputs(
            is_tty,
            stderr_is_tty,
            width,
            env::var_os("NO_COLOR").is_some(),
            no_banner.as_deref(),
            term.as_deref(),
        )
    }

    pub const fn banner_context(
        self,
        is_root_help: bool,
        is_root_without_task: bool,
    ) -> BannerContext {
        BannerContext {
            is_tty: self.is_tty,
            terminal_width: self.width,
            no_color: self.no_color,
            no_banner: self.no_banner,
            term_is_dumb: self.term_is_dumb,
            is_root_help,
            is_root_without_task,
        }
    }

    pub const fn color_enabled(self) -> bool {
        self.is_tty && !self.no_color && !self.term_is_dumb
    }

    pub const fn progress_is_tty(self) -> bool {
        self.stderr_is_tty
    }

    pub(crate) fn from_inputs(
        is_tty: bool,
        stderr_is_tty: bool,
        width: Option<u16>,
        no_color_present: bool,
        no_banner_value: Option<&str>,
        term: Option<&str>,
    ) -> Self {
        Self {
            is_tty,
            stderr_is_tty,
            width,
            no_color: no_color_present,
            no_banner: no_banner_value == Some("1"),
            term_is_dumb: term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalCapabilities;

    #[test]
    fn environment_values_are_interpreted_without_overmatching() {
        let capabilities =
            TerminalCapabilities::from_inputs(true, true, Some(120), true, Some("1"), Some("DuMb"));
        assert!(capabilities.no_color);
        assert!(capabilities.no_banner);
        assert!(capabilities.term_is_dumb);
        assert!(!capabilities.color_enabled());

        let capabilities =
            TerminalCapabilities::from_inputs(true, false, Some(80), false, Some("true"), None);
        assert!(!capabilities.no_banner);
        assert!(capabilities.color_enabled());
    }

    #[test]
    fn terminal_facts_map_directly_into_banner_decision_context() {
        let capabilities =
            TerminalCapabilities::from_inputs(true, true, Some(99), false, None, None);
        let context = capabilities.banner_context(true, false);
        assert!(context.is_tty);
        assert_eq!(context.terminal_width, Some(99));
        assert!(context.is_root_help);
        assert!(!context.is_root_without_task);
    }

    #[test]
    fn progress_requires_a_stderr_tty() {
        assert!(
            TerminalCapabilities::from_inputs(true, true, Some(99), false, None, None)
                .progress_is_tty()
        );
        assert!(
            !TerminalCapabilities::from_inputs(true, false, Some(99), false, None, None)
                .progress_is_tty()
        );
    }
}
