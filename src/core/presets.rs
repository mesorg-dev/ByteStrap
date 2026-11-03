//! Пресеты-контролы (схема Voidstrap): не чипы, а дропдауны/тумблеры,
//! которые пишут один-два флага. `options[0]` = «Авто» → флаг удаляется.

use crate::core::flags;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Выпадающий список: options[i] → flags[i]; index 0 удаляет флаг.
    Combo,
    /// Тумблер: true → value, false → удалить.
    Toggle,
}

pub struct Preset {
    pub label_key: &'static str,
    pub desc_key: &'static str,
    pub kind: Kind,
    /// (флаг, значение) на каждый индекс combo; для toggle — один.
    pub map: &'static [(&'static str, &'static str)],
    /// Отображаемые варианты; для combo length == map.len()+1 (первый — Авто).
    /// Для toggle — ["", ""] не используются.
    pub options: &'static [&'static str],
}

/// Все пресеты. Порядок = порядок в UI.
pub const PRESETS: &[Preset] = &[
    Preset {
        label_key: "Pr_Fps",
        desc_key: "Pr_FpsDesc",
        kind: Kind::Combo,
        map: &[
            ("DFIntTaskSchedulerTargetFps", "60"),
            ("DFIntTaskSchedulerTargetFps", "120"),
            ("DFIntTaskSchedulerTargetFps", "144"),
            ("DFIntTaskSchedulerTargetFps", "240"),
            ("DFIntTaskSchedulerTargetFps", "360"),
        ],
        options: &["Auto", "60", "120", "144", "240", "360"],
    },
    Preset {
        label_key: "Pr_Msaa",
        desc_key: "Pr_MsaaDesc",
        kind: Kind::Combo,
        map: &[
            ("FIntDebugForceMSAASamples", "1"),
            ("FIntDebugForceMSAASamples", "2"),
            ("FIntDebugForceMSAASamples", "4"),
            ("FIntDebugForceMSAASamples", "8"),
        ],
        options: &["Auto", "1x", "2x", "4x", "8x"],
    },
    Preset {
        label_key: "Pr_Render",
        desc_key: "Pr_RenderDesc",
        kind: Kind::Combo,
        map: &[
            ("FFlagDebugGraphicsPreferVulkan", "true"),
            ("FFlagDebugGraphicsPreferD3D11", "true"),
        ],
        options: &["Auto", "Vulkan", "D3D11"],
    },
    Preset {
        label_key: "Pr_Lod",
        desc_key: "Pr_LodDesc",
        kind: Kind::Combo,
        map: &[
            ("DFIntCSGLevelOfDetailSwitchingDistance", "500"),
            ("DFIntCSGLevelOfDetailSwitchingDistance", "1000"),
            ("DFIntCSGLevelOfDetailSwitchingDistance", "5000"),
        ],
        options: &["Auto", "Low", "Medium", "High"],
    },
    Preset {
        label_key: "Pr_Grass",
        desc_key: "Pr_GrassDesc",
        kind: Kind::Combo,
        map: &[
            ("FIntFRMMaxGrassDistance", "0"),
            ("FIntFRMMaxGrassDistance", "150"),
            ("FIntFRMMaxGrassDistance", "500"),
        ],
        options: &["Auto", "Off", "Medium", "Far"],
    },
    Preset {
        label_key: "Pr_Zoom",
        desc_key: "Pr_ZoomDesc",
        kind: Kind::Combo,
        map: &[
            ("FIntCameraMaxZoomDistance", "20"),
            ("FIntCameraMaxZoomDistance", "40"),
            ("FIntCameraMaxZoomDistance", "1000"),
        ],
        options: &["Auto", "20", "40", "1000"],
    },
    Preset {
        label_key: "Pr_Dpi",
        desc_key: "Pr_DpiDesc",
        kind: Kind::Toggle,
        map: &[("DFFlagDisableDPIScale", "true")],
        options: &["", ""],
    },
    Preset {
        label_key: "Pr_PostFx",
        desc_key: "Pr_PostFxDesc",
        kind: Kind::Toggle,
        map: &[("FFlagDisablePostFx", "true")],
        options: &["", ""],
    },
];

/// Текущий индекс combo: первый совпавший флаг со значением, иначе 0 (Авто).
pub fn current_index(flags: &[(String, String)], p: &Preset) -> usize {
    match p.kind {
        Kind::Toggle => {
            let (flag, value) = p.map[0];
            let on = flags
                .iter()
                .any(|(n, v)| n.trim().eq_ignore_ascii_case(flag) && v.trim() == value);
            usize::from(on)
        }
        Kind::Combo => {
            for (i, (flag, value)) in p.map.iter().enumerate() {
                if flags
                    .iter()
                    .any(|(n, v)| n.trim().eq_ignore_ascii_case(flag) && v.trim() == *value)
                {
                    return i + 1;
                }
            }
            0
        }
    }
}

/// Применить выбор: combo → выставить выбранный флаг и снять соседние варианты
/// того же семейства; toggle → поставить/удалить. index 0 (Авто) = удалить всё.
pub fn apply(flags: &mut Vec<(String, String)>, p: &Preset, index: usize) {
    match p.kind {
        Kind::Toggle => {
            let (flag, value) = p.map[0];
            if index == 0 {
                remove_flag(flags, flag);
            } else {
                set_flag(flags, flag, value);
            }
        }
        Kind::Combo => {
            // Снять все флаги семейства (включая «соседние» варианты).
            let mut seen: Vec<&str> = Vec::new();
            for (flag, _) in p.map {
                if !seen.contains(flag) {
                    seen.push(flag);
                    remove_flag(flags, flag);
                }
            }
            if index > 0 {
                if let Some((flag, value)) = p.map.get(index - 1) {
                    set_flag(flags, flag, value);
                }
            }
        }
    }
}

fn set_flag(flags: &mut Vec<(String, String)>, name: &str, value: &str) {
    match flags.iter_mut().find(|(n, _)| n.trim().eq_ignore_ascii_case(name)) {
        Some(row) => row.1 = value.to_string(),
        None => flags.push((name.to_string(), value.to_string())),
    }
}

fn remove_flag(flags: &mut Vec<(String, String)>, name: &str) {
    flags.retain(|(n, _)| !n.trim().eq_ignore_ascii_case(name));
}

/// Доступность: registry-валидация значений (для тестов/ UI-подсказок).
#[allow(dead_code)]
pub fn validate_preset(p: &Preset) -> Result<(), String> {
    for (flag, value) in p.map {
        flags::validate(flag, value).map_err(|e| format!("{flag}: {e:?}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(label: &'static str) -> &'static Preset {
        PRESETS.iter().find(|p| p.label_key == label).unwrap()
    }

    #[test]
    fn combo_default_removes_flag() {
        let p = find("Pr_Fps");
        let mut f = vec![("DFIntTaskSchedulerTargetFps".into(), "60".into())];
        assert_eq!(current_index(&f, p), 1);
        apply(&mut f, p, 0);
        assert!(f.is_empty(), "Auto must remove the flag");
        assert_eq!(current_index(&f, p), 0);
    }

    #[test]
    fn combo_switch_replaces_family() {
        let p = find("Pr_Fps");
        let mut f = vec![("DFIntTaskSchedulerTargetFps".into(), "60".into())];
        apply(&mut f, p, 4); // 240
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].1, "240");
        assert_eq!(current_index(&f, p), 4);
    }

    #[test]
    fn render_combo_clears_sibling_flags() {
        let p = find("Pr_Render");
        let mut f = vec![
            ("FFlagDebugGraphicsPreferD3D11".into(), "true".into()),
            ("OtherFlag".into(), "1".into()),
        ];
        apply(&mut f, p, 1); // Vulkan
        assert!(!f.iter().any(|(n, _)| n.contains("D3D11")));
        assert!(f.iter().any(|(n, _)| n.contains("Vulkan")));
        assert!(f.iter().any(|(n, _)| n == "OtherFlag"), "foreign flags kept");
    }

    #[test]
    fn toggle_on_off() {
        let p = find("Pr_Dpi");
        let mut f = Vec::new();
        apply(&mut f, p, 1);
        assert_eq!(f.len(), 1);
        assert_eq!(current_index(&f, p), 1);
        apply(&mut f, p, 0);
        assert!(f.is_empty());
    }

    #[test]
    fn map_sizes_match_options() {
        for p in PRESETS {
            match p.kind {
                Kind::Combo => assert_eq!(
                    p.options.len(),
                    p.map.len() + 1,
                    "{}: options vs map",
                    p.label_key
                ),
                Kind::Toggle => assert_eq!(p.options.len(), 2),
            }
        }
    }
}
