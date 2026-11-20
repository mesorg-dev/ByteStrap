//! Управление геометрией окна Roblox: установка кастомного разрешения
//! и центрирование окна на экране при запуске игры.

#[cfg(windows)]
pub fn apply_geometry(target_w: Option<u32>, target_h: Option<u32>, center: bool) {
    if target_w.is_none() && target_h.is_none() && !center {
        return;
    }

    std::thread::spawn(move || {
        // Ждём появления окна Roblox до 20 секунд
        for _ in 0..80 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let hwnd = crate::core::antiafk::find_game_hwnd();
            if !hwnd.is_null() {
                // Даём окну 200мс на первичную инициализацию Direct3D/Vulkan
                std::thread::sleep(std::time::Duration::from_millis(200));
                unsafe {
                    let mut rect = windows_sys::Win32::Foundation::RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);

                    let cur_w = (rect.right - rect.left).max(100) as u32;
                    let cur_h = (rect.bottom - rect.top).max(100) as u32;

                    let w = target_w.unwrap_or(cur_w);
                    let h = target_h.unwrap_or(cur_h);

                    let screen_w = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                        windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
                    ) as u32;
                    let screen_h = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                        windows_sys::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
                    ) as u32;

                    let (x, y) = if center {
                        (
                            screen_w.saturating_sub(w) as i32 / 2,
                            screen_h.saturating_sub(h) as i32 / 2,
                        )
                    } else {
                        (rect.left, rect.top)
                    };

                    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        x,
                        y,
                        w as i32,
                        h as i32,
                        windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                            | windows_sys::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW,
                    );
                }
                break;
            }
        }
    });
}

#[cfg(not(windows))]
pub fn apply_geometry(_w: Option<u32>, _h: Option<u32>, _center: bool) {}
