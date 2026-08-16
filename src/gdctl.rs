//! Turns a resolved layout into a `gdctl set` invocation.

use crate::config::Layout;
use crate::resolve::{Resolved, fmt_scale};

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
    }

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Screen;
    use crate::state::Monitor;

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
}
