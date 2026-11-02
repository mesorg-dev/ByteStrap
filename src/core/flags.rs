use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // String-вариант — под будущие строковые флаги
pub enum FlagType {
    Boolean,
    Integer,
    String,
}

pub struct FlagInfo {
    pub name: &'static str,
    pub ty: FlagType,
    pub desc: &'static str,
    pub category: &'static str,
    #[allow(dead_code)]
    pub default: &'static str,
    pub allowed: &'static [&'static str],
    pub min: Option<i64>,
    pub max: Option<i64>,
}

macro_rules! flag {
    ($name:literal, $ty:expr, $desc:literal, $cat:literal, $def:literal) => {
        FlagInfo { name: $name, ty: $ty, desc: $desc, category: $cat, default: $def, allowed: &[], min: None, max: None }
    };
    ($name:literal, $ty:expr, $desc:literal, $cat:literal, $def:literal, allowed: $allowed:expr) => {
        FlagInfo { name: $name, ty: $ty, desc: $desc, category: $cat, default: $def, allowed: $allowed, min: None, max: None }
    };
    ($name:literal, $ty:expr, $desc:literal, $cat:literal, $def:literal, min: $min:expr, max: $max:expr) => {
        FlagInfo { name: $name, ty: $ty, desc: $desc, category: $cat, default: $def, allowed: &[], min: Some($min), max: Some($max) }
    };
}

use FlagType::{Boolean, Integer};

/// Allowlist известных FastFlags — типы, диапазоны, описания.
pub static REGISTRY: &[FlagInfo] = &[
    flag!("FFlagDebugForceFutureIsBrightPhase3", Boolean, "Force Future Is Bright (Phase 3) lighting", "Rendering", "false"),
    flag!("FFlagDebugForceFutureIsBrightPhase2", Boolean, "Force Future Is Bright (Phase 2/ShadowMap) lighting", "Rendering", "false"),
    flag!("DFFlagDebugRenderForceTechnologyVoxel", Boolean, "Force Voxel lighting technology", "Rendering", "false"),
    flag!("FFlagRenderNoLowFrmBloom", Boolean, "Disable bloom on low frame rate", "Rendering", "false"),
    flag!("FFlagDisablePostFx", Boolean, "Disable all post-processing effects", "Rendering", "false"),
    flag!("FIntDebugForceMSAASamples", Integer, "Force MSAA sample count (0=off, 2, 4, 8)", "Rendering", "0", allowed: &["0", "2", "4", "8"]),
    flag!("FFlagDebugGraphicsPreferVulkan", Boolean, "Prefer Vulkan over D3D11", "Rendering", "false"),
    flag!("DFIntTaskSchedulerTargetFps", Integer, "Target FPS cap for task scheduler (5-1000)", "Performance", "60", min: 5, max: 1000),
    flag!("DFIntCSGLevelOfDetailSwitchingDistance", Integer, "Distance for CSG level of detail switching", "Performance", "1000", min: 100, max: 10000),
    flag!("FIntFRMMaxGrassDistance", Integer, "Maximum grass render distance (0=disabled)", "Performance", "200", min: 0, max: 5000),
    flag!("FIntCameraMaxZoomDistance", Integer, "Maximum camera zoom distance", "Performance", "128", min: 10, max: 5000),
    flag!("DFFlagDisableDPIScale", Boolean, "Disable DPI scaling (native resolution)", "Performance", "false"),
    flag!("DFFlagDebugPrintDataPingBreakDown", Boolean, "Print detailed ping breakdown to log", "Network", "false"),
    flag!("FFlagDebugPrintNetworkStats", Boolean, "Print network statistics to log", "Network", "false"),
    flag!("FFlagDebugCheckAudio", Boolean, "Enable audio debugging checks", "Debug", "false"),
    flag!("FFlagDebugGraphicsDisableVulkan", Boolean, "Force disable Vulkan", "Debug", "false"),
    flag!("FFlagDebugGraphicsPreferD3D11", Boolean, "Prefer D3D11 over Vulkan", "Debug", "false"),
    flag!("FIntCameraMinZoomDistance", Integer, "Minimum camera zoom distance", "Camera", "0.5", min: 0, max: 100),
    flag!("FFlagUserCameraToggle", Boolean, "Enable camera toggle mode", "Camera", "false"),
    flag!("DFIntTextureMemoryLimitMB", Integer, "Texture memory limit in MB", "Memory", "512", min: 64, max: 4096),
    flag!("DFIntMeshPartMemoryLimitMB", Integer, "MeshPart memory limit in MB", "Memory", "256", min: 32, max: 2048),
    flag!("DFIntPhysicsSolverIterations", Integer, "Physics solver iterations", "Physics", "8", min: 1, max: 32),
    flag!("DFIntPhysicsMaxContacts", Integer, "Maximum physics contacts per frame", "Physics", "10000", min: 1000, max: 100000),
    flag!("FFlagEnableNewChat", Boolean, "Enable new chat system", "UI", "true"),
    flag!("FFlagLuaDebuggerEnabled", Boolean, "Enable Lua debugger", "Developer", "false"),
    flag!("FFlagDebugShowPingInMenu", Boolean, "Show ping in escape menu", "UI", "false"),
];

pub fn lookup(name: &str) -> Option<&'static FlagInfo> {
    REGISTRY.iter().find(|f| f.name == name)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlagError {
    NotBoolean,
    NotInteger,
    BelowMin,
    AboveMax,
    NotAllowed,
}

/// Валидация значения. Неизвестные флаги проходят как есть.
pub fn validate(name: &str, value: &str) -> Result<(), FlagError> {
    let Some(info) = lookup(name) else {
        return Ok(());
    };
    match info.ty {
        FlagType::Boolean => {
            if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
                Ok(())
            } else {
                Err(FlagError::NotBoolean)
            }
        }
        FlagType::Integer => {
            let Ok(v) = value.trim().parse::<i64>() else {
                return Err(FlagError::NotInteger);
            };
            if let Some(min) = info.min {
                if v < min {
                    return Err(FlagError::BelowMin);
                }
            }
            if let Some(max) = info.max {
                if v > max {
                    return Err(FlagError::AboveMax);
                }
            }
            if !info.allowed.is_empty() && !info.allowed.contains(&value.trim()) {
                return Err(FlagError::NotAllowed);
            }
            Ok(())
        }
        FlagType::String => {
            if !info.allowed.is_empty() && !info.allowed.contains(&value) {
                Err(FlagError::NotAllowed)
            } else {
                Ok(())
            }
        }
    }
}

pub fn hint(info: &FlagInfo) -> String {
    match info.ty {
        FlagType::Boolean => "true/false".into(),
        FlagType::Integer => {
            if let (Some(min), Some(max)) = (info.min, info.max) {
                format!("{min}..{max}")
            } else if !info.allowed.is_empty() {
                format!("{{{}}}", info.allowed.join("|"))
            } else {
                "integer".into()
            }
        }
        FlagType::String => {
            if !info.allowed.is_empty() {
                format!("{{{}}}", info.allowed.join("|"))
            } else {
                "string".into()
            }
        }
    }
}

pub fn flags_path(version_dir: &Path) -> PathBuf {
    version_dir.join("ClientSettings").join("ClientAppSettings.json")
}

/// Значения в памяти — строки; в JSON типы восстанавливаются эвристикой.
pub fn load(version_dir: &Path) -> BTreeMap<String, String> {
    let path = flags_path(version_dir);
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    let Some(obj) = json.as_object() else {
        return BTreeMap::new();
    };
    obj.iter()
        .map(|(k, v)| {
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), val)
        })
        .collect()
}

fn typed_value(raw: &str) -> serde_json::Value {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    if let Ok(n) = t.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if t.contains('.') || t.contains('e') || t.contains('E') {
        if let Ok(f) = t.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(f) {
                return serde_json::Value::Number(n);
            }
        }
    }
    serde_json::Value::String(raw.to_string())
}

pub fn save(version_dir: &Path, flags: &BTreeMap<String, String>) -> Result<()> {
    let path = flags_path(version_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let obj: serde_json::Map<String, serde_json::Value> = flags
        .iter()
        .filter(|(k, _)| !k.trim().is_empty())
        .map(|(k, v)| (k.trim().to_string(), typed_value(v)))
        .collect();
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(obj))?;
    std::fs::write(path, text)?;
    Ok(())
}

// --- Профили флагов ---

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Profiles {
    #[serde(default)]
    pub profiles: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

fn profiles_path(base: &Path) -> PathBuf {
    base.join("flag_profiles.json")
}

pub fn load_profiles(base: &Path) -> Profiles {
    std::fs::read_to_string(profiles_path(base))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_profiles(base: &Path, profiles: &Profiles) -> Result<()> {
    std::fs::create_dir_all(base)?;
    let text = serde_json::to_string_pretty(profiles)?;
    std::fs::write(profiles_path(base), text)?;
    Ok(())
}

pub fn profile_to_flags(profile: &BTreeMap<String, serde_json::Value>) -> Vec<(String, String)> {
    profile
        .iter()
        .map(|(k, v)| {
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), val)
        })
        .collect()
}

pub fn flags_to_profile(flags: &[(String, String)]) -> BTreeMap<String, serde_json::Value> {
    flags
        .iter()
        .map(|(k, v)| (k.clone(), typed_value(v)))
        .collect()
}

/// Разбор импортируемого JSON-объекта в пары (строковые значения).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_import(text: &str) -> Result<Vec<(String, String)>> {
    let json: serde_json::Value = serde_json::from_str(text)?;
    let obj = json
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("ожидался JSON-объект"))?;
    Ok(obj
        .iter()
        .map(|(k, v)| {
            let val = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), val)
        })
        .collect())
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn export_json(flags: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = flags.iter().collect();
    sorted.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    let obj: serde_json::Map<String, serde_json::Value> = sorted
        .iter()
        .map(|(k, v)| (k.clone(), typed_value(v)))
        .collect();
    serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_boolean() {
        assert_eq!(validate("FFlagDisablePostFx", "true"), Ok(()));
        assert_eq!(validate("FFlagDisablePostFx", "FALSE"), Ok(()));
        assert_eq!(validate("FFlagDisablePostFx", "yes"), Err(FlagError::NotBoolean));
    }

    #[test]
    fn validate_integer_range() {
        assert_eq!(validate("DFIntTaskSchedulerTargetFps", "60"), Ok(()));
        assert_eq!(validate("DFIntTaskSchedulerTargetFps", "3"), Err(FlagError::BelowMin));
        assert_eq!(validate("DFIntTaskSchedulerTargetFps", "2000"), Err(FlagError::AboveMax));
        assert_eq!(validate("DFIntTaskSchedulerTargetFps", "abc"), Err(FlagError::NotInteger));
    }

    #[test]
    fn validate_allowed_list() {
        assert_eq!(validate("FIntDebugForceMSAASamples", "4"), Ok(()));
        assert_eq!(validate("FIntDebugForceMSAASamples", "3"), Err(FlagError::NotAllowed));
    }

    #[test]
    fn unknown_flags_pass() {
        assert_eq!(validate("MyOwnFlag", "anything"), Ok(()));
    }

    #[test]
    fn hints() {
        let fps = lookup("DFIntTaskSchedulerTargetFps").unwrap();
        assert_eq!(hint(fps), "5..1000");
        let msaa = lookup("FIntDebugForceMSAASamples").unwrap();
        assert_eq!(hint(msaa), "{0|2|4|8}");
        let fx = lookup("FFlagDisablePostFx").unwrap();
        assert_eq!(hint(fx), "true/false");
    }

    #[test]
    fn save_load_roundtrip_typed() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path();
        let mut m = BTreeMap::new();
        m.insert("FFlagA".to_string(), "true".to_string());
        m.insert("FIntB".to_string(), "42".to_string());
        m.insert("FFloatC".to_string(), "1.5".to_string());
        m.insert("FStrD".to_string(), "hello".to_string());
        save(dir, &m).unwrap();

        let text = std::fs::read_to_string(flags_path(dir)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["FFlagA"], serde_json::Value::Bool(true));
        assert!(json["FIntB"].is_number());
        assert_eq!(json["FIntB"].as_i64(), Some(42));
        assert!(json["FFloatC"].is_number());
        assert_eq!(json["FStrD"].as_str(), Some("hello"));

        let loaded = load(dir);
        assert_eq!(loaded.get("FFlagA").map(String::as_str), Some("true"));
        assert_eq!(loaded.get("FIntB").map(String::as_str), Some("42"));
        assert_eq!(loaded.get("FFloatC").map(String::as_str), Some("1.5"));
        assert_eq!(loaded.get("FStrD").map(String::as_str), Some("hello"));
    }

    #[test]
    fn load_broken_file_is_empty() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("ClientSettings").join("ClientAppSettings.json"), "oops")
            .ok();
        // каталога может и не быть — всё равно пусто
        let dir = td.path();
        std::fs::create_dir_all(dir.join("ClientSettings")).unwrap();
        std::fs::write(flags_path(dir), "oops").unwrap();
        assert!(load(dir).is_empty());
    }

    #[test]
    fn parse_import_object() {
        let v = parse_import(r#"{"a": true, "b": "x", "c": 3}"#).unwrap();
        assert_eq!(v.len(), 3);
        assert!(v.contains(&("a".to_string(), "true".to_string())));
        assert!(v.contains(&("b".to_string(), "x".to_string())));
        assert!(v.contains(&("c".to_string(), "3".to_string())));
        assert!(parse_import("[1,2]").is_err());
        assert!(parse_import("не json").is_err());
    }

    #[test]
    fn export_sorted_valid_json() {
        let flags = vec![
            ("Zeta".to_string(), "1".to_string()),
            ("alpha".to_string(), "true".to_string()),
        ];
        let out = export_json(&flags);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(json["alpha"], serde_json::Value::Bool(true));
        assert_eq!(json["Zeta"].as_i64(), Some(1));
    }

    #[test]
    fn profiles_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let mut p = Profiles::default();
        p.profiles.insert(
            "perf".to_string(),
            flags_to_profile(&[("FIntX".to_string(), "7".to_string())]),
        );
        save_profiles(td.path(), &p).unwrap();
        let loaded = load_profiles(td.path());
        let flags = profile_to_flags(&loaded.profiles["perf"]);
        assert_eq!(flags, vec![("FIntX".to_string(), "7".to_string())]);
        // отсутствующий файл — пустые профили
        assert!(load_profiles(&td.path().join("nope")).profiles.is_empty());
    }
}
