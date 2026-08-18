//! Match declared screens to connected monitors and pick a concrete scale.

use crate::core::config::Layout;
use crate::core::error::ConfigError;
use crate::core::state::{Monitor, State};

const SCALE_EPSILON: f64 = 1e-6;

#[derive(Debug)]
pub struct Resolved<'a> {
    pub screen: &'a crate::core::config::Screen,
    pub monitor: &'a Monitor,
    /// The declared scale, snapped to the exact value the mode supports.
    pub scale: f64,
}

/// Matches declared screens to connected monitors.
///
/// Returns `(resolved, absent)`. A non-empty `absent` means this layout does
/// not describe the hardware that is currently plugged in — that is not an
/// error, just a layout that does not apply right now.
pub fn resolve<'a>(
    layout: &'a Layout,
    state: &'a State,
) -> Result<(Vec<Resolved<'a>>, Vec<String>), ConfigError> {
    let mut problems: Vec<String> = Vec::new();
    let mut resolved: Vec<Resolved<'a>> = Vec::new();
    let mut absent: Vec<String> = Vec::new();

    for screen in &layout.screens {
        let identity = screen.identity();
        let mut candidates: Vec<&Monitor> = state
            .monitors
            .iter()
            .filter(|m| m.identity() == identity)
            .collect();
        if let Some(connector) = &screen.connector {
            candidates.retain(|m| &m.connector == connector);
        }

        let monitor = match candidates.as_slice() {
            [] => {
                absent.push(screen.name.clone());
                continue;
            }
            [only] => *only,
            many => {
                let connectors: Vec<&str> = many.iter().map(|m| m.connector.as_str()).collect();
                problems.push(format!(
                    "[screens.{}] matches {} connected monitors ({}); add a 'connector' key to \
                     pick one",
                    screen.name,
                    many.len(),
                    connectors.join(", ")
                ));
                continue;
            }
        };

        let Some(mode) = monitor.mode(&screen.mode) else {
            problems.push(format!(
                "[screens.{}] mode {:?} is not supported by {}",
                screen.name,
                screen.mode,
                monitor.describe()
            ));
            continue;
        };

        let snapped = mode
            .supported_scales
            .iter()
            .copied()
            .find(|s| (s - screen.scale).abs() <= SCALE_EPSILON);
        let Some(snapped) = snapped else {
            let supported: Vec<String> = mode
                .supported_scales
                .iter()
                .map(|s| fmt_scale(*s))
                .collect();
            problems.push(format!(
                "[screens.{}] scale {} is not supported for mode {} (supported: {})",
                screen.name,
                fmt_scale(screen.scale),
                mode.id,
                supported.join(", ")
            ));
            continue;
        };

        resolved.push(Resolved {
            screen,
            monitor,
            scale: snapped,
        });
    }

    let mut by_connector: Vec<(&str, &str)> = Vec::new();
    for item in &resolved {
        let connector = item.monitor.connector.as_str();
        if let Some((_, first_name)) = by_connector.iter().find(|(c, _)| *c == connector) {
            problems.push(format!(
                "[screens.{}] and [screens.{first_name}] both resolve to {connector}",
                item.screen.name
            ));
        } else {
            by_connector.push((connector, item.screen.name.as_str()));
        }
    }

    if !problems.is_empty() {
        return Err(ConfigError::InvalidFieldValues(problems));
    }

    Ok((resolved, absent))
}

pub fn fmt_scale(value: f64) -> String {
    if value == value.trunc() {
        (value as i64).to_string()
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{ColorMode, RgbRange, Screen, Transform};
    use crate::core::state::Mode;

    fn mode(id: &str, scales: &[f64]) -> Mode {
        Mode {
            id: id.to_string(),
            width: 1920,
            height: 1080,
            refresh: 60.0,
            preferred_scale: 1.0,
            supported_scales: scales.to_vec(),
            is_current: true,
            is_preferred: true,
        }
    }

    fn monitor(
        connector: &str,
        vendor: &str,
        product: &str,
        serial: &str,
        modes: Vec<Mode>,
    ) -> Monitor {
        Monitor {
            connector: connector.to_string(),
            vendor: vendor.to_string(),
            product: product.to_string(),
            serial: serial.to_string(),
            display_name: String::new(),
            modes,
            color_mode: Some(ColorMode::Default),
            rgb_range: Some(RgbRange::Auto),
        }
    }

    fn screen(
        name: &str,
        vendor: &str,
        product: &str,
        serial: &str,
        mode: &str,
        connector: Option<&str>,
    ) -> Screen {
        Screen {
            name: name.to_string(),
            vendor: vendor.to_string(),
            product: product.to_string(),
            serial: serial.to_string(),
            mode: mode.to_string(),
            x: 0,
            y: 0,
            scale: 1.0,
            transform: Transform::Normal,
            primary: true,
            connector: connector.map(str::to_string),
            color_mode: None,
            rgb_range: None,
            luminance: None,
        }
    }

    fn state(monitors: Vec<Monitor>) -> State {
        State {
            monitors,
            logical_monitors: Vec::new(),
            layout_mode: None,
            supports_changing_layout_mode: false,
        }
    }

    #[test]
    fn resolves_a_single_matching_screen() {
        let layout = Layout {
            screens: vec![screen("main", "LHC", "P2710S", "0", "1920x1080@60", None)],
            layout_mode: None,
        };
        let state = state(vec![monitor(
            "DP-1",
            "LHC",
            "P2710S",
            "0",
            vec![mode("1920x1080@60", &[1.0])],
        )]);

        let (resolved, absent) = resolve(&layout, &state).expect("should resolve cleanly");
        assert!(absent.is_empty());
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].monitor.connector, "DP-1");
        assert_eq!(resolved[0].scale, 1.0);
    }

    #[test]
    fn reports_absent_screens_without_erroring() {
        let layout = Layout {
            screens: vec![screen("main", "LHC", "P2710S", "0", "1920x1080@60", None)],
            layout_mode: None,
        };
        let state = state(vec![]);

        let (resolved, absent) =
            resolve(&layout, &state).expect("absent hardware is not a config error");
        assert!(resolved.is_empty());
        assert_eq!(absent, vec!["main".to_string()]);
    }

    #[test]
    fn ambiguous_identity_without_connector_pin_is_an_error() {
        let layout = Layout {
            screens: vec![screen("main", "LHC", "P2710S", "0", "1920x1080@60", None)],
            layout_mode: None,
        };
        let state = state(vec![
            monitor(
                "DP-1",
                "LHC",
                "P2710S",
                "0",
                vec![mode("1920x1080@60", &[1.0])],
            ),
            monitor(
                "DP-2",
                "LHC",
                "P2710S",
                "0",
                vec![mode("1920x1080@60", &[1.0])],
            ),
        ]);

        let err = resolve(&layout, &state).unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("add a 'connector' key")));
    }

    #[test]
    fn connector_pin_breaks_the_tie() {
        let layout = Layout {
            screens: vec![screen(
                "main",
                "LHC",
                "P2710S",
                "0",
                "1920x1080@60",
                Some("DP-2"),
            )],
            layout_mode: None,
        };
        let state = state(vec![
            monitor(
                "DP-1",
                "LHC",
                "P2710S",
                "0",
                vec![mode("1920x1080@60", &[1.0])],
            ),
            monitor(
                "DP-2",
                "LHC",
                "P2710S",
                "0",
                vec![mode("1920x1080@60", &[1.0])],
            ),
        ]);

        let (resolved, absent) = resolve(&layout, &state).expect("the pin should disambiguate");
        assert!(absent.is_empty());
        assert_eq!(resolved[0].monitor.connector, "DP-2");
    }

    #[test]
    fn unsupported_mode_is_an_error() {
        let layout = Layout {
            screens: vec![screen("main", "LHC", "P2710S", "0", "9999x9999@1", None)],
            layout_mode: None,
        };
        let state = state(vec![monitor(
            "DP-1",
            "LHC",
            "P2710S",
            "0",
            vec![mode("1920x1080@60", &[1.0])],
        )]);

        let err = resolve(&layout, &state).unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("is not supported by")));
    }

    #[test]
    fn scale_snaps_to_the_closest_supported_value_within_epsilon() {
        let mut declared = screen("main", "LHC", "P2710S", "0", "1920x1080@60", None);
        declared.scale = 1.25;
        let layout = Layout {
            screens: vec![declared],
            layout_mode: None,
        };
        let state = state(vec![monitor(
            "DP-1",
            "LHC",
            "P2710S",
            "0",
            vec![mode("1920x1080@60", &[1.0, 1.25 + 1e-9, 1.5])],
        )]);

        let (resolved, _) = resolve(&layout, &state).expect("epsilon-close scale should snap");
        assert_eq!(resolved[0].scale, 1.25 + 1e-9);
    }

    #[test]
    fn two_declared_screens_resolving_to_the_same_connector_is_an_error() {
        let layout = Layout {
            screens: vec![
                screen("a", "LHC", "P2710S", "0", "1920x1080@60", None),
                screen("b", "GSM", "OTHER", "1", "1920x1080@60", None),
            ],
            layout_mode: None,
        };
        // Both declared screens end up pointing at the same connector because
        // the second monitor's identity does not exist, but its connector
        // clashes only matters once resolved — so give both the same monitor.
        let shared = monitor(
            "DP-1",
            "LHC",
            "P2710S",
            "0",
            vec![mode("1920x1080@60", &[1.0])],
        );
        let mut state = state(vec![shared.clone()]);
        state.monitors.push(Monitor {
            vendor: "GSM".to_string(),
            product: "OTHER".to_string(),
            serial: "1".to_string(),
            ..shared
        });

        let err = resolve(&layout, &state).unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("both resolve to")));
    }

    #[test]
    fn fmt_scale_drops_the_fractional_part_for_whole_numbers() {
        assert_eq!(fmt_scale(1.0), "1");
        assert_eq!(fmt_scale(1.25), "1.25");
    }
}
