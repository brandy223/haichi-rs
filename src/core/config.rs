//! Declarative layout, loaded and validated from a TOML file.

use std::path::{Path, PathBuf};

use toml::Value;

use crate::core::error::ConfigError;

/// `$XDG_CONFIG_HOME/haichi/config.toml`, falling back to
/// `~/.config/haichi/config.toml` when `XDG_CONFIG_HOME` is unset or empty —
/// used when `apply` is run without an explicit path, so it can be invoked
/// unconditionally from a login or hotplug hook.
pub fn default_path() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    config_home
        .unwrap_or_default()
        .join("haichi")
        .join("config.toml")
}

pub const TRANSFORMS: [&str; 8] = [
    "normal",
    "90",
    "180",
    "270",
    "flipped",
    "flipped-90",
    "flipped-180",
    "flipped-270",
];

/// Values accepted by `gdctl set --color-mode`, per gdctl(1).
pub const COLOR_MODES: [&str; 3] = ["default", "sdr-native", "bt2100"];

/// Values accepted by `gdctl set --rgb-range`, per gdctl(1).
pub const RGB_RANGES: [&str; 3] = ["auto", "full", "limited"];

const SCREEN_KEYS: [&str; 13] = [
    "vendor",
    "product",
    "serial",
    "connector",
    "mode",
    "x",
    "y",
    "scale",
    "transform",
    "primary",
    "color-mode",
    "rgb-range",
    "luminance",
];

#[derive(Debug, Clone)]
pub struct Screen {
    pub name: String,
    pub vendor: String,
    pub product: String,
    pub serial: String,
    pub mode: String,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: String,
    pub primary: bool,
    /// Optional pin, breaks identity ties.
    pub connector: Option<String>,
    /// `gdctl set --color-mode`: one of [`COLOR_MODES`]. `bt2100` is HDR.
    pub color_mode: Option<String>,
    /// `gdctl set --rgb-range`: one of [`RGB_RANGES`].
    pub rgb_range: Option<String>,
    /// `gdctl pref --luminance`, applied to whichever color mode ends up
    /// active after `apply`. A separate command from `set` — see
    /// `commands::apply::gdctl::build_pref_commands`.
    pub luminance: Option<f64>,
}

impl Screen {
    pub fn identity(&self) -> (String, String, String) {
        (
            self.vendor.clone(),
            self.product.clone(),
            self.serial.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub screens: Vec<Screen>,
    pub layout_mode: Option<String>,
}

/// Loosely stringifies a TOML scalar the way Python's `str()` would, so a
/// value like `transform = 270` (no quotes) still matches `"270"`.
fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        other => other.to_string(),
    }
}

// code-review: was duplicated once per field (transform, then color-mode,
// then rgb-range); factored out so a fourth enum-like field is one call
// instead of another copy of the same three lines.
fn validate_one_of(
    value: &str,
    field: &str,
    allowed: &[&str],
    where_: &str,
    problems: &mut Vec<String>,
) {
    if !allowed.contains(&value) {
        problems.push(format!(
            "{where_}: {field} {value:?} is not one of {}",
            allowed.join(", ")
        ));
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Boolean(b) => *b,
        Value::Integer(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Table(t) => !t.is_empty(),
        Value::Datetime(_) => true,
    }
}

/// Coerces a TOML value to `f64` if it's a plain number (booleans excluded —
/// `bool` overlaps `int` in TOML's type model no more than in Rust's, so this
/// is just a type check here). No side effects; callers report their own
/// problem on `None`, since what counts as "invalid" differs (an absent
/// field is fine for `number` but not for `luminance`).
fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// Coerces a TOML value to `f64` if it's a plain number (booleans excluded —
/// `bool` overlaps `int` in TOML's type model no more than in Rust's, so this
/// is just a type check here). If the value is absent or not a number, reports
/// a problem and returns the default.
fn number(
    problems: &mut Vec<String>,
    value: Option<&Value>,
    field: &str,
    where_: &str,
    default: f64,
) -> f64 {
    match value {
        None => default,
        Some(value) => as_number(value).unwrap_or_else(|| {
            problems.push(format!("{where_}: {field} must be a number, got {value}"));
            default
        }),
    }
}

pub fn load_layout(path: &Path) -> Result<Layout, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|e| ConfigError::FileReadError {
        path: path.display().to_string(),
        source: e,
    })?;
    let raw: Value = toml::from_str(&text).map_err(|e| ConfigError::InvalidFormat {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_layout(raw)
}

fn parse_layout(raw: Value) -> Result<Layout, ConfigError> {
    let mut problems: Vec<String> = Vec::new();

    let layout_mode = match raw.get("layout-mode") {
        None => None,
        Some(Value::String(s)) if s == "logical" || s == "physical" => Some(s.clone()),
        Some(other) => {
            problems.push(format!(
                "layout-mode must be 'logical' or 'physical', got {other}"
            ));
            None
        }
    };

    let screens_table = match raw.get("screens").and_then(Value::as_table) {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Err(ConfigError::EmptyConfig(
                "no [screens.*] tables found; nothing to apply".to_string(),
            ));
        }
    };

    let mut screens: Vec<Screen> = Vec::new();
    for (name, body) in screens_table {
        let where_ = format!("[screens.{name}]");
        let Some(body) = body.as_table() else {
            problems.push(format!("{where_}: must be a table"));
            continue;
        };

        let mut unknown: Vec<&str> = body
            .keys()
            .map(String::as_str)
            .filter(|k| !SCREEN_KEYS.contains(k))
            .collect();
        if !unknown.is_empty() {
            unknown.sort_unstable();
            problems.push(format!("{where_}: unknown key(s): {}", unknown.join(", ")));
        }

        let missing: Vec<&str> = ["vendor", "product", "serial", "mode"]
            .into_iter()
            .filter(|k| !body.contains_key(*k))
            .collect();
        if !missing.is_empty() {
            problems.push(format!(
                "{where_}: missing required key(s): {}",
                missing.join(", ")
            ));
            continue;
        }

        let transform = body
            .get("transform")
            .map(scalar_string)
            .unwrap_or_else(|| "normal".to_string());
        validate_one_of(&transform, "transform", &TRANSFORMS, &where_, &mut problems);

        let x = number(&mut problems, body.get("x"), "x", &where_, 0.0) as i32;
        let y = number(&mut problems, body.get("y"), "y", &where_, 0.0) as i32;
        let scale = number(&mut problems, body.get("scale"), "scale", &where_, 1.0);

        let primary = match body.get("primary") {
            None => false,
            Some(Value::Boolean(b)) => *b,
            Some(other) => {
                problems.push(format!("{where_}: primary must be true or false"));
                truthy(other)
            }
        };

        let color_mode = body.get("color-mode").map(scalar_string);
        if let Some(color_mode) = &color_mode {
            validate_one_of(
                color_mode,
                "color-mode",
                &COLOR_MODES,
                &where_,
                &mut problems,
            );
        }

        let rgb_range = body.get("rgb-range").map(scalar_string);
        if let Some(rgb_range) = &rgb_range {
            validate_one_of(rgb_range, "rgb-range", &RGB_RANGES, &where_, &mut problems);
        }

        // Uses `as_number` rather than `number`: unlike x/y/scale, an absent
        // luminance means `None` (no default to fall back to), and a
        // present-but-non-numeric value should report just the type error,
        // not that *and* a misleading "not > 0" on top of it.
        let luminance = match body.get("luminance") {
            None => None,
            Some(value) => match as_number(value) {
                Some(n) if n > 0.0 && n.is_finite() => Some(n),
                Some(_) => {
                    problems.push(format!(
                        "{where_}: luminance must be a finite number greater than 0"
                    ));
                    None
                }
                None => {
                    problems.push(format!("{where_}: luminance must be a number, got {value}"));
                    None
                }
            },
        };

        screens.push(Screen {
            name: name.clone(),
            vendor: scalar_string(&body["vendor"]),
            product: scalar_string(&body["product"]),
            serial: scalar_string(&body["serial"]),
            mode: scalar_string(&body["mode"]),
            x,
            y,
            scale,
            transform,
            primary,
            connector: body.get("connector").map(scalar_string),
            color_mode,
            rgb_range,
            luminance,
        });
    }

    let primaries: Vec<&str> = screens
        .iter()
        .filter(|s| s.primary)
        .map(|s| s.name.as_str())
        .collect();
    if primaries.len() > 1 {
        problems.push(format!(
            "exactly one screen must be primary, but these are: {}",
            primaries.join(", ")
        ));
    } else if !screens.is_empty() && primaries.is_empty() {
        problems.push("exactly one screen must be primary, but none is".to_string());
    }

    let mut seen: Vec<((String, String, String), &str)> = Vec::new();
    for screen in &screens {
        let identity = screen.identity();
        if let Some((_, first_name)) = seen.iter().find(|(id, _)| *id == identity) {
            if screen.connector.is_none() {
                problems.push(format!(
                    "[screens.{}] has the same identity as [screens.{first_name}]; pin each one \
                     with a 'connector' key to tell them apart",
                    screen.name
                ));
            }
        } else {
            seen.push((identity, &screen.name));
        }
    }

    if !problems.is_empty() {
        return Err(ConfigError::InvalidFieldValues(problems));
    }

    Ok(Layout {
        screens,
        layout_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Layout, ConfigError> {
        parse_layout(toml::from_str(text).expect("test fixture must be valid TOML"))
    }

    #[test]
    fn parses_a_minimal_valid_layout() {
        let layout = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0000000000000"
            mode = "2560x1440@240.002"
            primary = true
            "#,
        )
        .expect("valid layout should parse");

        assert_eq!(layout.screens.len(), 1);
        let screen = &layout.screens[0];
        assert_eq!(screen.vendor, "LHC");
        assert_eq!(screen.x, 0);
        assert_eq!(screen.scale, 1.0);
        assert_eq!(screen.transform, "normal");
        assert!(screen.primary);
        assert!(screen.connector.is_none());
    }

    #[test]
    fn rejects_missing_required_keys() {
        let err = parse("[screens.main]\nvendor = \"LHC\"\n").unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(
            problems
                .iter()
                .any(|p| p.contains("missing required key(s)"))
        );
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            bogus = 1
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("unknown key(s): bogus")));
    }

    #[test]
    fn requires_exactly_one_primary() {
        let none_primary = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &none_primary else {
            panic!("expected InvalidFieldValues, got: {none_primary:?}");
        };
        assert!(problems.iter().any(|p| p.contains("none is")));

        let two_primary = parse(
            r#"
            [screens.a]
            vendor = "LHC"
            product = "P"
            serial = "1"
            mode = "1920x1080@60"
            primary = true

            [screens.b]
            vendor = "LHC"
            product = "P"
            serial = "2"
            mode = "1920x1080@60"
            primary = true
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &two_primary else {
            panic!("expected InvalidFieldValues, got: {two_primary:?}");
        };
        assert!(problems.iter().any(|p| p.contains("but these are")));
    }

    #[test]
    fn duplicate_identity_requires_a_connector_pin() {
        let err = parse(
            r#"
            [screens.a]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true

            [screens.b]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("pin each one")));

        // Screens are visited in table-key order ("a" before "b"). The check
        // only looks at the *current* screen's own connector, so pinning "b"
        // — the one visited after its identity is already known — is what
        // silences the warning, not pinning "a".
        parse(
            r#"
            [screens.a]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true

            [screens.b]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            connector = "DP-2"
            "#,
        )
        .expect("pinning the later-visited screen should break the identity tie");
    }

    #[test]
    fn rejects_unknown_transform() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            transform = "sideways"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("transform")));
    }

    #[test]
    fn accepts_an_unquoted_numeric_transform() {
        // `transform = 270` (no quotes) still matches the string "270", the
        // same loose coercion the original Python tool applied via `str()`.
        let layout = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            transform = 270
            "#,
        )
        .expect("numeric transform should coerce to a string");
        assert_eq!(layout.screens[0].transform, "270");
    }

    #[test]
    fn rejects_non_numeric_x() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            x = "not-a-number"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("x must be a number")));
    }

    #[test]
    fn reports_every_problem_on_a_screen_not_just_the_first() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            x = "not-a-number"
            y = "also-not-a-number"
            color-mode = "vivid"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("x must be a number")));
        assert!(problems.iter().any(|p| p.contains("y must be a number")));
        assert!(problems.iter().any(|p| p.contains("color-mode")));
    }

    #[test]
    fn rejects_empty_layout() {
        let err = parse("").unwrap_err();
        assert!(matches!(err, ConfigError::EmptyConfig(_)));
    }

    #[test]
    fn parses_color_mode_rgb_range_and_luminance() {
        let layout = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            color-mode = "bt2100"
            rgb-range = "full"problems.iter().any(|p| p.contains("no [screens.*] tables"))
            luminance = 400
            "#,
        )
        .expect("valid color settings should parse");

        let screen = &layout.screens[0];
        assert_eq!(screen.color_mode.as_deref(), Some("bt2100"));
        assert_eq!(screen.rgb_range.as_deref(), Some("full"));
        assert_eq!(screen.luminance, Some(400.0));
    }

    #[test]
    fn color_mode_rgb_range_and_luminance_default_to_none() {
        let layout = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            "#,
        )
        .expect("valid layout should parse");

        let screen = &layout.screens[0];
        assert!(screen.color_mode.is_none());
        assert!(screen.rgb_range.is_none());
        assert!(screen.luminance.is_none());
    }

    #[test]
    fn rejects_unknown_color_mode() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            color-mode = "vivid"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("color-mode")));
    }

    #[test]
    fn rejects_unknown_rgb_range() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            rgb-range = "wide"
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(problems.iter().any(|p| p.contains("rgb-range")));
    }

    #[test]
    fn rejects_non_positive_luminance() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            luminance = 0
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(
            problems
                .iter()
                .any(|p| p.contains("luminance must be a finite number greater than 0"))
        );
    }

    #[test]
    fn rejects_non_finite_luminance() {
        let err = parse(
            r#"
            [screens.main]
            vendor = "LHC"
            product = "P2710S"
            serial = "0"
            mode = "1920x1080@60"
            primary = true
            luminance = inf
            "#,
        )
        .unwrap_err();
        let ConfigError::InvalidFieldValues(problems) = &err else {
            panic!("expected InvalidFieldValues, got: {err:?}");
        };
        assert!(
            problems
                .iter()
                .any(|p| p.contains("luminance must be a finite number greater than 0"))
        );
    }
}
