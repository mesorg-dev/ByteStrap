//! Anti-AFK: пока идёт игровая сессия, раз в минуту имитируем нажатие пробела
//! в окне Roblox, чтобы клиент не отключал игрока за неактивность.
//!
//! Только Windows (PostMessage в окно RobloxPlayerBeta); на других платформах
//! запуск/остановка — no-op.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Как часто «нажимаем» пробел. Roblox считает игрока активным заметно реже.
#[cfg_attr(not(windows), allow(dead_code))]
const PERIOD: Duration = Duration::from_secs(60);

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
            while !s.load(Ordering::Relaxed) {
                // Ждём период короткими шагами, чтобы быстро реагировать на stop.
                let steps = PERIOD.as_millis() as u64 / 200;
                for _ in 0..steps {
                    if s.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                imp::press_space();
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
mod imp {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, IsWindowVisible, PostMessageW, WM_KEYDOWN, WM_KEYUP,
    };

    const GAME_EXE: &str = "robloxplayerbeta.exe";
    const VK_SPACE: usize = 0x20;

    /// Отправить пробел в игровое окно (если оно найдено).
    pub fn press_space() {
        let hwnd = find_game_hwnd();
        if hwnd.is_null() {
            return;
        }
        unsafe {
            PostMessageW(hwnd, WM_KEYDOWN, VK_SPACE, 0);
            PostMessageW(hwnd, WM_KEYUP, VK_SPACE, 0);
        }
    }

    /// Видимое топ-левел окно процесса RobloxPlayerBeta.
    fn find_game_hwnd() -> HWND {
        extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> i32 {
            unsafe {
                let out = lparam as *mut HWND;
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let mut pid: u32 = 0;
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);
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
