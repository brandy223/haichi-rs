//! Current display state, read straight from `org.gnome.Mutter.DisplayConfig`.
//!
//! `gdctl show` is never parsed — its output is human-facing and
//! reformattable, while `GetCurrentState` is the stable D-Bus contract.

use std::collections::BTreeMap;

use serde::Deserialize;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Type};

use crate::core::error::AppError;

const LAYOUT_MODE_LOGICAL: u32 = 1;
const LAYOUT_MODE_PHYSICAL: u32 = 2;

pub type Identity = (String, String, String);

#[derive(Debug, Clone, PartialEq)]
pub struct Mode {
    pub id: String,
    pub width: i32,
    pub height: i32,
    pub refresh: f64,
    #[allow(dead_code)] // part of the D-Bus contract; not needed by any command yet
    pub preferred_scale: f64,
    pub supported_scales: Vec<f64>,
    pub is_current: bool,
    #[allow(dead_code)]
    pub is_preferred: bool,
}

#[derive(Debug, Clone)]
pub struct Monitor {
    pub connector: String,
    pub vendor: String,
    pub product: String,
    pub serial: String,
    pub display_name: String,
    pub modes: Vec<Mode>,
}

impl Monitor {
    pub fn identity(&self) -> Identity {
        (
            self.vendor.clone(),
            self.product.clone(),
            self.serial.clone(),
        )
    }

    pub fn current_mode(&self) -> Option<&Mode> {
        self.modes.iter().find(|m| m.is_current)
    }

    pub fn mode(&self, mode_id: &str) -> Option<&Mode> {
        self.modes.iter().find(|m| m.id == mode_id)
    }

    pub fn describe(&self) -> String {
        format!(
            "{} ({} {} serial={:?})",
            self.connector, self.vendor, self.product, self.serial
        )
    }
}

/// A monitor spec as it appears in a logical monitor's `specs` list:
/// `(connector, vendor, product, serial)`.
pub type MonitorSpec = (String, String, String, String);

#[derive(Debug, Clone)]
pub struct LogicalMonitor {
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: u32,
    pub primary: bool,
    pub specs: Vec<MonitorSpec>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub monitors: Vec<Monitor>,
    pub logical_monitors: Vec<LogicalMonitor>,
    pub layout_mode: Option<String>,
    pub supports_changing_layout_mode: bool,
}

// --------------------------------------------------------------------------
// Wire types: one struct per D-Bus STRUCT in GetCurrentState's reply
// signature, `(ua((ssss)a(siiddada{sv})a{sv})a(iiduba(ssss)a{sv})a{sv})`.
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Type, Deserialize)]
struct RawMode {
    id: String,
    width: i32,
    height: i32,
    refresh: f64,
    preferred_scale: f64,
    supported_scales: Vec<f64>,
    properties: BTreeMap<String, OwnedValue>,
}

#[derive(Debug, Clone, Type, Deserialize)]
struct RawMonitorSpec {
    connector: String,
    vendor: String,
    product: String,
    serial: String,
}

#[derive(Debug, Clone, Type, Deserialize)]
struct RawMonitorEntry {
    spec: RawMonitorSpec,
    modes: Vec<RawMode>,
    properties: BTreeMap<String, OwnedValue>,
}

#[derive(Debug, Clone, Type, Deserialize)]
struct RawLogicalMonitor {
    x: i32,
    y: i32,
    scale: f64,
    transform: u32,
    primary: bool,
    monitors: Vec<RawMonitorSpec>,
    #[allow(dead_code)]
    // must stay to match the wire signature; no logical-monitor property is read
    properties: BTreeMap<String, OwnedValue>,
}

type GetCurrentStateReply = (
    u32,
    Vec<RawMonitorEntry>,
    Vec<RawLogicalMonitor>,
    BTreeMap<String, OwnedValue>,
);

fn prop_bool(props: &BTreeMap<String, OwnedValue>, key: &str) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn prop_str(props: &BTreeMap<String, OwnedValue>, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or_default()
        .to_string()
}

pub fn read_state() -> Result<State, AppError> {
    let connection = Connection::session()?;
    let proxy = Proxy::new(
        &connection,
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
    )?;
    let (_serial, raw_monitors, raw_logical, props): GetCurrentStateReply =
        proxy.call("GetCurrentState", &())?;

    let monitors = raw_monitors
        .into_iter()
        .map(|entry| Monitor {
            display_name: prop_str(&entry.properties, "display-name"),
            connector: entry.spec.connector,
            vendor: entry.spec.vendor,
            product: entry.spec.product,
            serial: entry.spec.serial,
            modes: entry
                .modes
                .into_iter()
                .map(|raw| Mode {
                    is_current: prop_bool(&raw.properties, "is-current"),
                    is_preferred: prop_bool(&raw.properties, "is-preferred"),
                    id: raw.id,
                    width: raw.width,
                    height: raw.height,
                    refresh: raw.refresh,
                    preferred_scale: raw.preferred_scale,
                    supported_scales: raw.supported_scales,
                })
                .collect(),
        })
        .collect();

    let logical_monitors = raw_logical
        .into_iter()
        .map(|lm| LogicalMonitor {
            x: lm.x,
            y: lm.y,
            scale: lm.scale,
            transform: lm.transform,
            primary: lm.primary,
            specs: lm
                .monitors
                .into_iter()
                .map(|s| (s.connector, s.vendor, s.product, s.serial))
                .collect(),
        })
        .collect();

    let layout_mode = match props.get("layout-mode").and_then(|v| u32::try_from(v).ok()) {
        Some(LAYOUT_MODE_LOGICAL) => Some("logical".to_string()),
        Some(LAYOUT_MODE_PHYSICAL) => Some("physical".to_string()),
        _ => None,
    };

    Ok(State {
        monitors,
        logical_monitors,
        layout_mode,
        supports_changing_layout_mode: prop_bool(&props, "supports-changing-layout-mode"),
    })
}
