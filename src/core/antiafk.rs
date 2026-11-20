//! Anti-AFK: предотвращает отключение клиента Roblox за неактивность (Error 260).
//!
//! Если игрок активно взаимодействует с игрой (нажимает клавиши, двигает мышь),
//! эмуляция клавиш НЕ происходит, чтобы не мешать игровому процессу (например, случайными прыжками).
//! Нажатие пробела отправляется только тогда, когда пользователь неактивен дольше заданного
//! порога (IDLE_THRESHOLD).
//!
//! Только Windows (PostMessage в окно RobloxPlayerBeta); на других платформах
//! запуск/остановка — no-op.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

/// Порог неактивности игрока, после которого разрешено имитировать активность (5 минут).
/// Roblox отключает за неактивность через 20 минут.
#[cfg_attr(not(windows), allow(dead_code))]
const IDLE_THRESHOLD: Duration = Duration::from_secs(5 * 60);

/// Частота отправки пробела в состоянии AFK.
#[cfg_attr(not(windows), allow(dead_code))]
const PRESS_INTERVAL: Duration = Duration::from_secs(60);

/// Интервал проверки активности (каждые 200 мс для быстрой реакции на остановку).
#[cfg_attr(not(windows), allow(dead_code))]
const CHECK_STEP: Duration = Duration::from_millis(200);

/// Хэндл запущенного потока anti-AFK; `stop()` завершает его.
pub struct Handle {
    stop: Arc<AtomicBool>,
}

impl Handle {
    /// Остановить анти-AFK.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Запустить anti-AFK. Возвращённый хэндл нужно сохранить: при его уничтожении
/// поток останавливается.
pub fn start() -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg(windows)]
    {
        let s = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut last_press = Instant::now();
            let mut last_interaction = Instant::now();

            while !s.load(Ordering::Relaxed) {
                // Короткий сон, чтобы моментально реагировать на выключение (сумма 2 секунды)
                for _ in 0..10 {
                    if s.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(CHECK_STEP);
                }

                let game_hwnd = imp::find_game_hwnd();
                if game_hwnd.is_null() {
                    continue;
                }

                let is_foreground = imp::is_game_foreground(game_hwnd);
                let idle_duration = imp::get_idle_time();

                if is_foreground {
                    // Игра на переднем плане. Если пользователь трогал клавиатуру или мышь,
                    // обновляем время последнего взаимодействия.
                    if let Some(idle) = idle_duration {
                        if idle < IDLE_THRESHOLD {
                            last_interaction = Instant::now();
                        }
                    }
                }

                let time_since_interaction = last_interaction.elapsed();
                let user_is_idle = match idle_duration {
                    Some(idle) => idle >= IDLE_THRESHOLD,
                    None => false,
                };

                // Нажимаем пробел только если:
                // 1) Игра в фокусе, но пользователь физически AFK >= IDLE_THRESHOLD;
                // 2) Или игра в фоне, и в ней не было действий >= IDLE_THRESHOLD.
                let should_press = (is_foreground && user_is_idle)
                    || (!is_foreground && time_since_interaction >= IDLE_THRESHOLD);

                if should_press && last_press.elapsed() >= PRESS_INTERVAL {
                    imp::press_space_hwnd(game_hwnd);
                    last_press = Instant::now();
                }
            }
        });
    }
    #[cfg(not(windows))]
    {
        let _ = &stop;
    }
    Handle { stop }
}

#[cfg(windows)]
pub use imp::find_game_hwnd;

#[cfg(windows)]
mod imp {
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
        WM_KEYDOWN, WM_KEYUP,
    };

    const GAME_EXE: &str = "robloxplayerbeta.exe";
    const VK_SPACE: usize = 0x20;

    /// Возвращает время, прошедшее с момента последнего ввода пользователя в системе (клавиатура/мышь).
    pub fn get_idle_time() -> Option<Duration> {
        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        unsafe {
            if GetLastInputInfo(&mut lii) != 0 {
                let now = GetTickCount();
                let idle_ms = now.wrapping_sub(lii.dwTime);
                Some(Duration::from_millis(idle_ms as u64))
            } else {
                None
            }
        }
    }

    /// Проверяет, активно ли сейчас окно игры (или дочернее/модальное окно того же процесса).
    pub fn is_game_foreground(game_hwnd: HWND) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return false;
            }
            if fg == game_hwnd {
                return true;
            }
            let mut fg_pid: u32 = 0;
            GetWindowThreadProcessId(fg, &mut fg_pid);
            let mut game_pid: u32 = 0;
            GetWindowThreadProcessId(game_hwnd, &mut game_pid);
            fg_pid != 0 && fg_pid == game_pid
        }
    }

    /// Отправить пробел в указанное игровое окно.
    pub fn press_space_hwnd(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            PostMessageW(hwnd, WM_KEYDOWN, VK_SPACE, 0);
            PostMessageW(hwnd, WM_KEYUP, VK_SPACE, 0);
        }
    }

    /// Отправить пробел в игровое окно (если оно найдено).
    #[allow(dead_code)]
    pub fn press_space() {
        let hwnd = find_game_hwnd();
        press_space_hwnd(hwnd);
    }

    /// Видимое топ-левел окно процесса RobloxPlayerBeta.
    pub fn find_game_hwnd() -> HWND {
        extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> i32 {
            unsafe {
                let out = lparam as *mut HWND;
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == 0 {
                    return 1;
                }
                let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if proc.is_null() {
                    return 1;
                }
                let mut buf = [0u16; 260];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(proc, 0, buf.as_mut_ptr(), &mut len);
                CloseHandle(proc);
                if ok != 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    let name = path
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if name == GAME_EXE {
                        *out = hwnd;
                        return 0;
                    }
                }
                1
            }
        }

        let mut found: HWND = std::ptr::null_mut();
        unsafe {
            EnumWindows(Some(cb), &mut found as *mut HWND as LPARAM);
        }
        found
    }
}
