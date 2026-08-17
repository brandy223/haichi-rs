//! Turns a resolved layout into `gdctl` invocations.

use crate::core::config::Layout;
use crate::core::resolve::{Resolved, fmt_scale};

pub fn build_command(
    resolved: &[Resolved],
    layout: &Layout,
    persistent: bool,
    verify: bool,
) -> Vec<String> {
    let mut cmd = vec!["gdctl".to_string(), "set".to_string()];
    if persistent {
        cmd.push("--persistent".to_string());
    }
    if verify {
        cmd.push("--verify".to_string());
    }
    if let Some(mode) = &layout.layout_mode {
        cmd.push("--layout-mode".to_string());
        cmd.push(mode.clone());
    }

    for item in resolved {
        let screen = item.screen;
        cmd.push("--logical-monitor".to_string());
        cmd.push("-x".to_string());
        cmd.push(screen.x.to_string());
        cmd.push("-y".to_string());
        cmd.push(screen.y.to_string());
        cmd.push("--scale".to_string());
        cmd.push(fmt_scale(item.scale));
        cmd.push("--transform".to_string());
        cmd.push(screen.transform.clone());
        if screen.primary {
            cmd.push("--primary".to_string());
        }
        cmd.push("--monitor".to_string());
        cmd.push(item.monitor.connector.clone());
        cmd.push("--mode".to_string());
        cmd.push(screen.mode.clone());
        if let Some(color_mode) = &screen.color_mode {
            cmd.push("--color-mode".to_string());
            cmd.push(color_mode.clone());
        }
        if let Some(rgb_range) = &screen.rgb_range {
            cmd.push("--rgb-range".to_string());
            cmd.push(rgb_range.clone());
        }
    }

    cmd
}

/// Builds one `gdctl pref` invocation per resolved screen that declares a
/// luminance. `pref` is a separate command from `set` — per gdctl(1),
/// `--luminance` applies to "the current color mode", so these must run
/// *after* the `set` command that establishes it.
pub fn build_pref_commands(resolved: &[Resolved]) -> Vec<Vec<String>> {
    resolved
        .iter()
        .filter_map(|item| {
            let luminance = item.screen.luminance?;
            Some(vec![
                "gdctl".to_string(),
                "pref".to_string(),
                "--monitor".to_string(),
                item.monitor.connector.clone(),
                "--luminance".to_string(),
                fmt_scale(luminance),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Screen;
    use crate::core::state::Monitor;

    fn screen(name: &str, primary: bool) -> Screen {
        Screen {
            name: name.to_string(),
            vendor: "LHC".to_string(),
            product: "P2710S".to_string(),
            serial: "0".to_string(),
            mode: "2560x1440@240.002".to_string(),
            x: 0,
            y: 0,
            scale: 1.25,
            transform: "270".to_string(),
            primary,
            connector: None,
            color_mode: None,
            rgb_range: None,
            luminance: None,
        }
    }

    fn monitor(connector: &str) -> Monitor {
        Monitor {
            connector: connector.to_string(),
            vendor: "LHC".to_string(),
            product: "P2710S".to_string(),
            serial: "0".to_string(),
            display_name: String::new(),
            modes: Vec::new(),
            color_mode: "default".to_string(),
            rgb_range: "auto".to_string(),
        }
    }

    #[test]
    fn builds_a_persistent_command_with_layout_mode_and_primary() {
        let screen = screen("main", true);
        let monitor = monitor("DP-9");
        let resolved = [Resolved {
            screen: &screen,
            monitor: &monitor,
            scale: 1.25,
        }];
        let layout = Layout {
            screens: vec![],
            layout_mode: Some("logical".to_string()),
        };

        let cmd = build_command(&resolved, &layout, true, false);

        assert_eq!(
            cmd,
            vec![
                "gdctl",
                "set",
                "--persistent",
                "--layout-mode",
                "logical",
                "--logical-monitor",
                "-x",
                "0",
                "-y",
                "0",
                "--scale",
                "1.25",
                "--transform",
                "270",
                "--primary",
                "--monitor",
                "DP-9",
                "--mode",
                "2560x1440@240.002",
            ]
        );
    }

    #[test]
    fn verify_omits_persistent_and_a_non_primary_screen_omits_the_flag() {
        let screen = screen("main", false);
        let monitor = monitor("DP-9");
        let resolved = [Resolved {
            screen: &screen,
            monitor: &monitor,
            scale: 1.0,
        }];
        let layout = Layout {
            screens: vec![],
            layout_mode: None,
        };

        let cmd = build_command(&resolved, &layout, false, true);

        assert!(!cmd.contains(&"--persistent".to_string()));
        assert!(cmd.contains(&"--verify".to_string()));
        assert!(!cmd.contains(&"--primary".to_string()));
        assert!(!cmd.contains(&"--layout-mode".to_string()));
    }

    #[test]
    fn color_mode_and_rgb_range_are_appended_after_mode_when_declared() {
        let mut screen = screen("main", true);
        screen.color_mode = Some("bt2100".to_string());
        screen.rgb_range = Some("full".to_string());
        let monitor = monitor("DP-9");
        let resolved = [Resolved {
            screen: &screen,
            monitor: &monitor,
            scale: 1.25,
        }];
        let layout = Layout {
            screens: vec![],
            layout_mode: None,
        };

        let cmd = build_command(&resolved, &layout, true, false);

        let mode_pos = cmd.iter().position(|a| a == "--mode").unwrap();
        assert_eq!(cmd[mode_pos + 2], "--color-mode");
        assert_eq!(cmd[mode_pos + 3], "bt2100");
        assert_eq!(cmd[mode_pos + 4], "--rgb-range");
        assert_eq!(cmd[mode_pos + 5], "full");
    }

    #[test]
    fn color_mode_and_rgb_range_are_omitted_when_not_declared() {
        let screen = screen("main", true);
        let monitor = monitor("DP-9");
        let resolved = [Resolved {
            screen: &screen,
            monitor: &monitor,
            scale: 1.25,
        }];
        let layout = Layout {
            screens: vec![],
            layout_mode: None,
        };

        let cmd = build_command(&resolved, &layout, true, false);

        assert!(!cmd.contains(&"--color-mode".to_string()));
        assert!(!cmd.contains(&"--rgb-range".to_string()));
    }

    #[test]
    fn build_pref_commands_emits_one_luminance_command_per_declared_screen() {
        let mut with_luminance = screen("bright", true);
        with_luminance.luminance = Some(400.0);
        let without_luminance = screen("dim", false);
        let bright_monitor = monitor("DP-9");
        let dim_monitor = monitor("DP-10");
        let resolved = [
            Resolved {
                screen: &with_luminance,
                monitor: &bright_monitor,
                scale: 1.0,
            },
            Resolved {
                screen: &without_luminance,
                monitor: &dim_monitor,
                scale: 1.0,
            },
        ];

        let cmds = build_pref_commands(&resolved);

        assert_eq!(
            cmds,
            vec![vec![
                "gdctl".to_string(),
                "pref".to_string(),
                "--monitor".to_string(),
                "DP-9".to_string(),
                "--luminance".to_string(),
                "400".to_string(),
            ]]
        );
    }
}
