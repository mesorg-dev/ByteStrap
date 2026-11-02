use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub fn resolve(saved: &str) -> Self {
        match saved {
            "ru" => Lang::Ru,
            "en" => Lang::En,
            _ => system_lang(),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }
}

fn system_lang() -> Lang {
    #[cfg(windows)]
    {
        if let Ok(v) = std::env::var("LANG") {
            if v.to_lowercase().starts_with("ru") {
                return Lang::Ru;
            }
        }
        // GetUserDefaultUILanguage: 0x0419 = ru-RU
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        unsafe {
            if GetUserDefaultUILanguage() == 0x0419 {
                Lang::Ru
            } else {
                Lang::En
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(v) = std::env::var("LANG") {
            if v.to_lowercase().starts_with("ru") {
                return Lang::Ru;
            }
        }
        Lang::En
    }
}

macro_rules! dict {
    ($($k:literal => $v:literal),* $(,)?) => {{
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        $(m.insert($k, $v);)*
        m
    }};
}

static RU: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| dict! {
    // Навигация
    "Nav_Home" => "Главная",
    "Nav_Flags" => "FastFlags",
    "Nav_Versions" => "Версии",
    "Nav_About" => "О программе",
    "App_Sub" => "R O B L O X",
    "Footer_Ready" => "готов",
    "Footer_Busy" => "занят",
    // Окно
    "Win_Min" => "Свернуть",
    "Win_Max" => "Развернуть",
    "Win_Close" => "Закрыть",
    // Общее
    "Btn_OK" => "ОК",
    "Btn_Cancel" => "Отмена",
    "Btn_Close" => "Закрыть",
    "Btn_Copy" => "Копировать",
    // Главная
    "Home_H1" => "Главная",
    "Home_Sub" => "Установка и запуск клиента",
    "Home_Play" => "Играть",
    "Home_Install" => "Установить",
    "Home_Update" => "Обновить",
    "Home_Status_Ready" => "Готово к запуску",
    "Home_Status_NoClient" => "Клиент не установлен",
    "Home_Status_Checking" => "Проверка версии…",
    "Home_Status_Downloading" => "Загрузка {0}%",
    "Home_Status_Extracting" => "Распаковка {0}/{1}",
    "Home_Status_Setup" => "Финализация…",
    "Home_Status_Update" => "Доступна версия {0}",
    "Home_Status_Launching" => "Запуск…",
    "Home_Status_Error" => "Ошибка: {0}",
    "Home_Version" => "Версия клиента {0}",
    "Home_VersionNone" => "клиент не установлен",
    "Home_MbOf" => "{0} / {1} МБ",
    // FastFlags
    "Flags_H1" => "FastFlags",
    "Flags_Sub" => "Флаги клиентской настройки",
    "Flags_Search" => "Поиск по имени…",
    "Flags_Add" => "Добавить",
    "Flags_Save" => "Сохранить",
    "Flags_Import" => "Импорт",
    "Flags_Paste" => "Из буфера",
    "Flags_Export" => "Экспорт",
    "Flags_Delete" => "Удалить",
    "Flags_Clear" => "Удалить всё",
    "Flags_Empty" => "Нет флагов. Добавьте или импортируйте JSON.",
    "Flags_Presets" => "Пресеты",
    "Flags_Profiles" => "Профили",
    "Flags_SaveProfile" => "Сохранить профиль",
    "Flags_DeleteProfile" => "Удалить профиль",
    "Flags_ProfileName" => "Имя профиля…",
    "Flags_ConflictTitle" => "Конфликт значения",
    "Flags_ConflictMsg" => "{0}: «{1}» → «{2}»",
    "Flags_ConflictKeep" => "Оставить",
    "Flags_ConflictReplace" => "Заменить",
    "Flags_ConflictReplaceAll" => "Заменить все",
    "Flags_ColName" => "Имя",
    "Flags_ColValue" => "Значение",
    "Flags_ColDesc" => "Описание",
    "Flags_ClearConfirm" => "Удалить все флаги из таблицы?",
    "Flags_Saved" => "Флаги сохранены",
    "Flags_Imported" => "Импортировано: {0}, пропущено: {1}",
    "Flags_SaveErr" => "Не вышло сохранить флаги: ",
    "Flags_ImportErr" => "Не вышло импортировать: ",
    "Flags_ExportErr" => "Не вышло экспортировать: ",
    "Flags_ExportedFmt" => "Экспортировано флагов: {0}",
    "Flags_EmptyImport" => "В импорте нет ни одного флага.",
    "Flags_NoClipboard" => "В буфере обмена нет JSON.",
    "Flags_Cleared" => "Все флаги удалены (нажми «Сохранить»)",
    "Flags_ProfilesEmpty" => "Пока пусто — собери флаги выше и сохрани как профиль.",
    "Search_Empty" => "Ничего не нашлось. Попробуй иначе.",
    "Empty_Flags" => "Флагов нет — добавьте первый или примените пресет.",
    "Btn_Yes" => "Да",
    "Btn_No" => "Нет",
    "Dlg_FlagsImport" => "Импорт флагов из JSON",
    "Dlg_FlagsExport" => "Экспорт флагов в JSON",
    "Dlg_AllFiles" => "Все файлы",
    "FlagErr_NotBoolFmt" => "Логический флаг ждёт true/false, получено «{0}»",
    "FlagErr_NotIntFmt" => "Числовой флаг, получено «{0}»",
    "FlagErr_MinFmt" => "Минимум {1}, получено «{0}»",
    "FlagErr_MaxFmt" => "Максимум {1}, получено «{0}»",
    "FlagErr_AllowedFmt" => "Допустимо: {1}, получено «{0}»",
    "Prof_ApplyConfirm" => "Применить профиль «{0}»? Таблица заменится целиком.",
    "Prof_AppliedFmt" => "Профиль «{0}»: {1} флагов (нажми «Сохранить»)",
    "Prof_NeedName" => "Введи название профиля.",
    "Prof_EmptyTable" => "Таблица пустая — нечего сохранять.",
    "Prof_SavedFmt" => "Профиль «{0}» сохранён",
    "Prof_NoSuchFmt" => "Профиля «{0}» нет.",
    "Prof_DelConfirm" => "Удалить профиль «{0}»?",
    "Prof_DeletedFmt" => "Профиль «{0}» удалён",
    // Версии
    "Ver_H1" => "Версии",
    "Ver_Sub" => "Установленные клиенты",
    "Ver_Activate" => "Активировать",
    "Ver_Delete" => "Удалить",
    "Ver_Check" => "Проверить файлы",
    "Ver_Repair" => "Восстановить",
    "Ver_Empty" => "Нет установленных версий",
    "Ver_ColGuid" => "Версия",
    "Ver_ColSize" => "Размер",
    "Ver_ColStatus" => "Статус",
    "Ver_Active" => "активна",
    "Ver_IntegrityOk" => "Файлы в порядке",
    "Ver_IntegrityIssues" => "Проблем: {0}",
    "Ver_DeleteConfirm" => "Удалить версию {0}?",
    "Ver_NoActive" => "нет активной",
    // О программе
    "About_H1" => "О программе",
    "About_Sub" => "Версия и обновления",
    "About_Version" => "Версия",
    "About_CheckUpdates" => "Проверить обновления",
    "About_Checking" => "Проверка…",
    "About_UpToDate" => "Установлена последняя версия",
    "About_UpdateAvail" => "Доступна версия {0}",
    "About_Download" => "Скачать и обновить",
    "About_Downloading" => "Загрузка обновления…",
    "About_UpdateErr" => "Не удалось проверить обновления: {0}",
    "About_Lang" => "Язык",
    "About_Repo" => "Репозиторий",
    "About_License" => "Лицензия",
});

static EN: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| dict! {
    // Navigation
    "Nav_Home" => "Home",
    "Nav_Flags" => "FastFlags",
    "Nav_Versions" => "Versions",
    "Nav_About" => "About",
    "App_Sub" => "R O B L O X",
    "Footer_Ready" => "ready",
    "Footer_Busy" => "busy",
    // Window
    "Win_Min" => "Minimize",
    "Win_Max" => "Maximize",
    "Win_Close" => "Close",
    // Common
    "Btn_OK" => "OK",
    "Btn_Cancel" => "Cancel",
    "Btn_Close" => "Close",
    "Btn_Copy" => "Copy",
    // Home
    "Home_H1" => "Home",
    "Home_Sub" => "Install and launch the client",
    "Home_Play" => "Play",
    "Home_Install" => "Install",
    "Home_Update" => "Update",
    "Home_Status_Ready" => "Ready to launch",
    "Home_Status_NoClient" => "Client not installed",
    "Home_Status_Checking" => "Checking version…",
    "Home_Status_Downloading" => "Downloading {0}%",
    "Home_Status_Extracting" => "Extracting {0}/{1}",
    "Home_Status_Setup" => "Finalizing…",
    "Home_Status_Update" => "Update {0} available",
    "Home_Status_Launching" => "Launching…",
    "Home_Status_Error" => "Error: {0}",
    "Home_Version" => "Client version {0}",
    "Home_VersionNone" => "client not installed",
    "Home_MbOf" => "{0} / {1} MB",
    // FastFlags
    "Flags_H1" => "FastFlags",
    "Flags_Sub" => "Client settings flags",
    "Flags_Search" => "Search by name…",
    "Flags_Add" => "Add",
    "Flags_Save" => "Save",
    "Flags_Import" => "Import",
    "Flags_Paste" => "From clipboard",
    "Flags_Export" => "Export",
    "Flags_Delete" => "Delete",
    "Flags_Clear" => "Delete all",
    "Flags_Empty" => "No flags. Add or import JSON.",
    "Flags_Presets" => "Presets",
    "Flags_Profiles" => "Profiles",
    "Flags_SaveProfile" => "Save profile",
    "Flags_DeleteProfile" => "Delete profile",
    "Flags_ProfileName" => "Profile name…",
    "Flags_ConflictTitle" => "Value conflict",
    "Flags_ConflictMsg" => "{0}: \"{1}\" → \"{2}\"",
    "Flags_ConflictKeep" => "Keep",
    "Flags_ConflictReplace" => "Replace",
    "Flags_ConflictReplaceAll" => "Replace all",
    "Flags_ColName" => "Name",
    "Flags_ColValue" => "Value",
    "Flags_ColDesc" => "Description",
    "Flags_ClearConfirm" => "Delete all flags from the table?",
    "Flags_Saved" => "Flags saved",
    "Flags_Imported" => "Imported: {0}, skipped: {1}",
    "Flags_SaveErr" => "Couldn't save flags: ",
    "Flags_ImportErr" => "Couldn't import: ",
    "Flags_ExportErr" => "Couldn't export: ",
    "Flags_ExportedFmt" => "Exported {0} flags",
    "Flags_EmptyImport" => "The import contains no flags.",
    "Flags_NoClipboard" => "No JSON in the clipboard.",
    "Flags_Cleared" => "All flags deleted (hit \u{201c}Save\u{201d})",
    "Flags_ProfilesEmpty" => "Empty yet — build flags above and save as a profile.",
    "Search_Empty" => "Nothing found. Try another word.",
    "Empty_Flags" => "No flags yet — add one or apply a preset.",
    "Btn_Yes" => "Yes",
    "Btn_No" => "No",
    "Dlg_FlagsImport" => "Import flags from JSON",
    "Dlg_FlagsExport" => "Export flags to JSON",
    "Dlg_AllFiles" => "All files",
    "FlagErr_NotBoolFmt" => "A boolean flag wants true/false, got \u{201c}{0}\u{201d}",
    "FlagErr_NotIntFmt" => "A numeric flag, got \u{201c}{0}\u{201d}",
    "FlagErr_MinFmt" => "Minimum {1}, got \u{201c}{0}\u{201d}",
    "FlagErr_MaxFmt" => "Maximum {1}, got \u{201c}{0}\u{201d}",
    "FlagErr_AllowedFmt" => "Allowed: {1}, got \u{201c}{0}\u{201d}",
    "Prof_ApplyConfirm" => "Apply profile \u{201c}{0}\u{201d}? The table will be replaced.",
    "Prof_AppliedFmt" => "Profile \u{201c}{0}\u{201d}: {1} flags (hit \u{201c}Save\u{201d})",
    "Prof_NeedName" => "Enter a profile name.",
    "Prof_EmptyTable" => "Table is empty — nothing to save.",
    "Prof_SavedFmt" => "Profile \u{201c}{0}\u{201d} saved",
    "Prof_NoSuchFmt" => "No profile \u{201c}{0}\u{201d}.",
    "Prof_DelConfirm" => "Delete profile \u{201c}{0}\u{201d}?",
    "Prof_DeletedFmt" => "Profile \u{201c}{0}\u{201d} deleted",
    // Versions
    "Ver_H1" => "Versions",
    "Ver_Sub" => "Installed clients",
    "Ver_Activate" => "Activate",
    "Ver_Delete" => "Delete",
    "Ver_Check" => "Check files",
    "Ver_Repair" => "Repair",
    "Ver_Empty" => "No installed versions",
    "Ver_ColGuid" => "Version",
    "Ver_ColSize" => "Size",
    "Ver_ColStatus" => "Status",
    "Ver_Active" => "active",
    "Ver_IntegrityOk" => "Files OK",
    "Ver_IntegrityIssues" => "Issues: {0}",
    "Ver_DeleteConfirm" => "Delete version {0}?",
    "Ver_NoActive" => "none",
    // About
    "About_H1" => "About",
    "About_Sub" => "Version and updates",
    "About_Version" => "Version",
    "About_CheckUpdates" => "Check for updates",
    "About_Checking" => "Checking…",
    "About_UpToDate" => "You are on the latest version",
    "About_UpdateAvail" => "Version {0} available",
    "About_Download" => "Download and update",
    "About_Downloading" => "Downloading update…",
    "About_UpdateErr" => "Couldn't check for updates: {0}",
    "About_Lang" => "Language",
    "About_Repo" => "Repository",
    "About_License" => "License",
});

pub fn tr(lang: Lang, key: &'static str) -> &'static str {
    let m = match lang {
        Lang::Ru => &*RU,
        Lang::En => &*EN,
    };
    m.get(key)
        .copied()
        .or_else(|| EN.get(key).copied())
        .unwrap_or(key)
}

pub fn fmt(lang: Lang, key: &'static str, args: &[&str]) -> String {
    let mut s = tr(lang, key).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}
